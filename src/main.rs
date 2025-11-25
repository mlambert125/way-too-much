use futures::lock::{Mutex, MutexGuard};
use memmap2::{MmapMut, MmapOptions, RemapOptions};
use sendfd::RecvWithFd;
use std::{
    collections::{HashMap, VecDeque},
    fmt::Display,
    sync::Arc,
};
use tokio::{
    io::AsyncWriteExt,
    net::{UnixSocket, UnixStream},
};
use tracing::{debug, error, warn};

struct CompositorGlobalState {
    globals: Vec<(u32, WaylandObject, u32)>,
}
impl Default for CompositorGlobalState {
    fn default() -> Self {
        CompositorGlobalState {
            globals: vec![
                (1, WaylandObject::Shm, 1),
                (2, WaylandObject::WlCompositor, 6),
                (3, WaylandObject::XdgWmBase, 7),
            ],
        }
    }
}

struct CompositorClientState<'a> {
    stream: &'a mut UnixStream,
    object_registry: HashMap<u32, WaylandObject>,
}
impl<'a> CompositorClientState<'a> {
    fn new(stream: &'a mut UnixStream) -> Self {
        let mut object_registry = HashMap::new();
        object_registry.insert(1, WaylandObject::Display);
        CompositorClientState {
            object_registry,
            stream,
        }
    }
}

enum WaylandObject {
    Display,
    Registry,

    XdgWmBase,
    ShmPool(Arc<Mutex<MmapMut>>, i32),
    WlCompositor,

    Callback,
    Shm,
    Buffer(Buffer),
    WlSurface,
    WlRegion,
}
impl WaylandObject {
    fn as_str(&self) -> &'static str {
        match self {
            WaylandObject::Display => "wl_display",
            WaylandObject::Registry => "wl_registry",
            WaylandObject::Callback => "wl_callback",
            WaylandObject::XdgWmBase => "xdg_wm_base",
            WaylandObject::ShmPool(_, _) => "wl_shm_pool",
            WaylandObject::Shm => "wl_shm",
            WaylandObject::Buffer(_) => "wl_buffer",
            WaylandObject::WlCompositor => "wl_compositor",
            WaylandObject::WlSurface => "wl_surface",
            WaylandObject::WlRegion => "wl_region",
        }
    }
}
impl Display for WaylandObject {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

enum WlShmFormat {
    Argb8888 = 0,
    Rgb888 = 0x34324752,
}

struct Buffer {
    offset: i32,
    width: i32,
    height: i32,
    stride: i32,
    format: u32,
    shm_pool: Arc<Mutex<MmapMut>>,
}

async fn get_string_bytes(s: &str) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&(s.len() as u32 + 1).to_le_bytes());
    bytes.extend_from_slice(s.as_bytes());
    bytes.push(0);
    while bytes.len() % 4 != 0 {
        bytes.push(0);
    }
    bytes
}

impl<'a> CompositorClientState<'a> {
    async fn send_message(
        &mut self,
        object_id: u32,
        op_code: u16,
        args: &[u8],
    ) -> anyhow::Result<()> {
        if self.stream.writable().await.is_err() {
            error!("Failed to await writability on socket");
            anyhow::bail!("Socket not writable");
        }
        self.stream.write_all(&object_id.to_le_bytes()).await?;
        self.stream.write_all(&op_code.to_le_bytes()).await?;
        self.stream
            .write_all(&(8 + args.len() as u16).to_le_bytes())
            .await?;
        self.stream.write_all(args).await?;
        Ok(())
    }

    async fn send_callback_done(
        &mut self,
        callback_id: u32,
        callback_data: u32,
    ) -> anyhow::Result<()> {
        let argument_bytes = callback_data.to_le_bytes();
        debug!("Sending callback done event for id {}", callback_id);
        self.send_message(callback_id, 0, &argument_bytes).await
    }

    async fn send_global(
        &mut self,
        registry_id: u32,
        name: u32,
        interface: &str,
        version: u32,
    ) -> anyhow::Result<()> {
        let mut args = Vec::new();
        args.extend_from_slice(&name.to_le_bytes());

        let interface_bytes = get_string_bytes(interface).await;
        args.extend_from_slice(&interface_bytes);
        args.extend_from_slice(&version.to_le_bytes());

        debug!(
            "Sending global {} (interface: {}, version: {}) to registry id {}",
            name, interface, version, registry_id
        );

        self.send_message(registry_id, 0, &args).await
    }

    async fn send_format(&mut self, shm_id: u32, format: u32) -> anyhow::Result<()> {
        let argument_bytes = format.to_le_bytes();
        debug!("Sending shm format event for id {}", shm_id);
        self.send_message(shm_id, 0, &argument_bytes).await
    }

    async fn handle_message(
        &mut self,
        object_id: u32,
        op_code: u16,
        arg_bytes: &[u8],
        fds: &mut VecDeque<i32>,
        global_state: MutexGuard<'_, CompositorGlobalState>,
    ) -> anyhow::Result<()> {
        if let Some(object) = self.object_registry.get_mut(&object_id) {
            match object {
                WaylandObject::Display => match op_code {
                    // wl_display.sync(callback: wl_callback)
                    0 => {
                        let new_id = u32::from_le_bytes(arg_bytes[..4].try_into().unwrap());
                        debug!("Display sync called with new_id {}", new_id);

                        self.object_registry.insert(new_id, WaylandObject::Callback);
                        self.send_callback_done(new_id, 0).await?;
                    }
                    // wl_display.get_registry(registry:new_id<wl_registry>)
                    1 => {
                        let new_id = u32::from_le_bytes(arg_bytes[..4].try_into().unwrap());
                        debug!("Display get_registry called with new_id {}", new_id);
                        self.object_registry.insert(new_id, WaylandObject::Registry);

                        for (name, interface, version) in &global_state.globals {
                            self.send_global(new_id, *name, interface.as_str(), *version)
                                .await?;
                        }
                    }
                    _ => {
                        warn!("Unknown op_code {} for Display", op_code);
                    }
                },
                WaylandObject::Registry => match op_code {
                    // wl_registry.bind(name:u32, id:new_id)
                    0 => {
                        let name = u32::from_le_bytes(arg_bytes[0..4].try_into().unwrap());
                        let iface_len =
                            u32::from_le_bytes(arg_bytes[4..8].try_into().unwrap()) as usize;
                        let padded_len = (iface_len + 3) & !3;
                        let interface =
                            String::from_utf8(arg_bytes[8..8 + iface_len - 1].to_vec()).unwrap();

                        let version = u32::from_le_bytes(
                            arg_bytes[8 + padded_len..12 + padded_len]
                                .try_into()
                                .unwrap(),
                        );
                        let new_id = u32::from_le_bytes(
                            arg_bytes[12 + padded_len..16 + padded_len]
                                .try_into()
                                .unwrap(),
                        );
                        debug!(
                            "Registry bind called with name={}, new_id=({}::{}:{})",
                            name, interface, version, new_id
                        );

                        if let Some((_, interface, version)) =
                            global_state.globals.iter().find(|(n, _, _)| *n == name)
                        {
                            let object = match interface {
                                WaylandObject::Shm => WaylandObject::Shm,
                                WaylandObject::XdgWmBase => WaylandObject::XdgWmBase,
                                WaylandObject::WlCompositor => WaylandObject::WlCompositor,
                                _ => {
                                    anyhow::bail!(
                                        "Unknown interface requested from globals: {}",
                                        interface
                                    );
                                }
                            };

                            if let WaylandObject::Shm = object {
                                self.send_format(new_id, WlShmFormat::Argb8888 as u32)
                                    .await?;
                                self.send_format(new_id, WlShmFormat::Rgb888 as u32).await?;
                            }

                            self.object_registry.insert(new_id, object);
                            debug!(
                                "Bound new object id {} for interface {} version {}",
                                new_id, interface, version
                            );
                        } else {
                            warn!("No global found with name {}", name);
                        }
                    }
                    _ => {
                        warn!("Unknown op_code {} for Registry", op_code);
                    }
                },
                WaylandObject::Callback => {
                    warn!("Callback does not have any op codes to handle");
                }
                WaylandObject::Shm => match op_code {
                    // wl_shm.create_pool(id:new_id<wl_sm_pool>, fd:fd, size:u32)
                    0 => {
                        debug!("Creating shm pool");
                        let new_id = u32::from_le_bytes(arg_bytes[0..4].try_into().unwrap());
                        let size = i32::from_le_bytes(arg_bytes[4..8].try_into().unwrap());
                        let fd = fds.pop_front();

                        if let Some(fd) = fd {
                            // mmap size bytes of the passed in fd
                            let mmap =
                                unsafe { MmapOptions::new().len(size as usize).map_mut(fd)? };
                            self.object_registry.insert(
                                new_id,
                                WaylandObject::ShmPool(Arc::new(Mutex::new(mmap)), fd),
                            );
                        } else {
                            anyhow::bail!("No file descriptor provided for shm pool creation");
                        }
                    }
                    // wl_shm.release()
                    1 => {
                        debug!("Releasing shm pool");
                        self.object_registry.remove(&object_id);
                    }
                    _ => {
                        warn!("Unknown op_code {} for Shm", op_code);
                    }
                },
                WaylandObject::ShmPool(mmap, _fd) => match op_code {
                    // wl_shm_pool.create_buffer(id:new_id<wl_buffer>, offset:u32, width:u32, height:u32, stride:u32, format:u32<wl_shm.format>)
                    0 => {
                        debug!("ShmPool.create_buffer called");
                        let new_id = u32::from_le_bytes(arg_bytes[0..4].try_into().unwrap());
                        let offset = i32::from_le_bytes(arg_bytes[4..8].try_into().unwrap());
                        let width = i32::from_le_bytes(arg_bytes[8..12].try_into().unwrap());
                        let height = i32::from_le_bytes(arg_bytes[12..16].try_into().unwrap());
                        let stride = i32::from_le_bytes(arg_bytes[16..20].try_into().unwrap());
                        let format = u32::from_le_bytes(arg_bytes[20..24].try_into().unwrap());
                        let buffer = Buffer {
                            offset,
                            width,
                            height,
                            stride,
                            format,
                            shm_pool: mmap.clone(),
                        };
                        self.object_registry
                            .insert(new_id, WaylandObject::Buffer(buffer));
                    }
                    // wl_shm_pool.destroy()
                    1 => {
                        debug!("ShmPool.destroy called");
                        self.object_registry.remove(&object_id);
                    }
                    // wl_shm_pool.resize(size:u32)
                    2 => {
                        debug!("ShmPool.resize called");
                        let new_size = u32::from_le_bytes(arg_bytes[0..4].try_into().unwrap());
                        let mut mmap = mmap.lock().await;
                        unsafe {
                            mmap.remap(new_size as usize, RemapOptions::new().may_move(false))?;
                        }
                    }
                    _ => {
                        warn!("Unknown op_code {} for ShmPool", op_code);
                    }
                },
                WaylandObject::Buffer(_) => match op_code {
                    // wl_buffer.destroy()
                    0 => {
                        debug!("Buffer.destroy called");
                        self.object_registry.remove(&object_id);
                    }
                    _ => {
                        warn!("Unknown op_code {} for Buffer", op_code);
                    }
                },

                WaylandObject::WlCompositor => match op_code {
                    // wl_compositor.create_surface(id:new_id<wl_surface>)
                    0 => {
                        let new_id = u32::from_le_bytes(arg_bytes[..4].try_into().unwrap());
                        debug!("WlCompositor.create_surface called with new_id {}", new_id);
                        self.object_registry
                            .insert(new_id, WaylandObject::WlSurface);
                    }
                    // wl_compositor.create_region(id:new_id<wl_region>)
                    1 => {
                        let new_id = u32::from_le_bytes(arg_bytes[..4].try_into().unwrap());
                        debug!("WlCompositor.create_region called with new_id {}", new_id);
                        self.object_registry.insert(new_id, WaylandObject::WlRegion);
                    }
                    _ => {
                        warn!("Unknown op_code {} for WlCompositor", op_code);
                    }
                },

                WaylandObject::WlSurface => {
                    warn!("No op_codes implemented for WlSurface");
                }

                WaylandObject::WlRegion => {
                    warn!("No op_codes implemented for WlRegion");
                }

                WaylandObject::XdgWmBase => {
                    warn!("No op_codes implemented for XdgWmBase");
                }
            }
            Ok(())
        } else {
            warn!("Unknown object ID: {}", object_id);
            Ok(())
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .init();

    let global_state_mutex = Arc::new(Mutex::new(CompositorGlobalState::default()));

    let socket_path = "/tmp/my-wayland-socket.sock";
    let _ = std::fs::remove_file(socket_path);

    let socket = UnixSocket::new_stream()?;
    socket.bind(socket_path)?;

    let listener = socket.listen(1024)?;
    println!("Listening on {:?}", socket_path);

    loop {
        let (mut stream, _) = listener.accept().await?;
        let global_state_mutex = global_state_mutex.clone();

        tokio::spawn(async move {
            debug!("New client connected");
            let mut client_state = CompositorClientState::new(&mut stream);
            if client_state.stream.readable().await.is_err() {
                error!("Failed to await readability on socket");
                return;
            }

            let mut data = VecDeque::<u8>::new();
            let mut pending_fds = VecDeque::<i32>::new();

            loop {
                let mut buffer = [0u8; 4096];
                let mut fds = [0; 10];
                let result = client_state.stream.recv_with_fd(&mut buffer, &mut fds);

                //let result = client_state.stream.read(&mut buffer).await.map(|n| (n, 0));

                match result {
                    Ok((0, 0)) => {
                        warn!("Connection closed while reading");
                        return;
                    }
                    Ok((data_read, fds_read)) => {
                        debug!("Read {} bytes and {} fds", data_read, fds_read);
                        for byte in &buffer[..data_read] {
                            data.push_back(*byte);
                        }
                        for &fd in &fds[..fds_read] {
                            pending_fds.push_back(fd);
                        }

                        while data.len() >= 8
                            && data.len() >= u16::from_le_bytes([data[6], data[7]]) as usize
                        {
                            let object_id = u32::from_le_bytes([
                                data.pop_front().unwrap(),
                                data.pop_front().unwrap(),
                                data.pop_front().unwrap(),
                                data.pop_front().unwrap(),
                            ]);
                            let op_code = u16::from_le_bytes([
                                data.pop_front().unwrap(),
                                data.pop_front().unwrap(),
                            ]) as usize;
                            let message_length = u16::from_le_bytes([
                                data.pop_front().unwrap(),
                                data.pop_front().unwrap(),
                            ]);
                            let mut args_buffer = vec![0u8; message_length as usize - 8];
                            (0..args_buffer.len()).for_each(|i| {
                                args_buffer[i] = data.pop_front().unwrap();
                            });
                            let global_state = global_state_mutex.lock().await;
                            let res = client_state
                                .handle_message(
                                    object_id,
                                    op_code as u16,
                                    &args_buffer,
                                    &mut pending_fds,
                                    global_state,
                                )
                                .await;
                            if let Err(e) = res {
                                error!("Error handling message: {}", e);
                                error!("Closing connection due to error.");
                                stream.shutdown().await.ok();
                                return;
                            }
                        }
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        continue;
                    }
                    Err(e) => {
                        warn!("Connection closed or error while reading: {}", e);
                        return;
                    }
                }
            }
        });
    }
}
