use futures::lock::{Mutex, MutexGuard};
use memmap2::MmapMut;
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

use crate::{wl_buffer::BufferState, wl_surface::SurfaceState};

mod utils;
mod wl_buffer;
mod wl_callback;
mod wl_compositor;
mod wl_display;
mod wl_output;
mod wl_region;
mod wl_registry;
mod wl_shm;
mod wl_shm_pool;
mod wl_surface;
mod xdg_wm_base;

struct CompositorGlobalState {
    globals: Vec<(u32, WaylandObject, u32)>,
}
impl Default for CompositorGlobalState {
    fn default() -> Self {
        CompositorGlobalState {
            globals: vec![
                (1, WaylandObject::WlShm, 1),
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
        object_registry.insert(1, WaylandObject::WlDisplay);
        CompositorClientState {
            object_registry,
            stream,
        }
    }
}

enum WaylandObject {
    WlDisplay,
    WlRegistry,

    XdgWmBase,
    WlShmPool(Arc<Mutex<MmapMut>>, i32),
    WlCompositor,

    WlCallback,
    WlShm,
    WlBuffer(BufferState),
    WlSurface(SurfaceState),
    WlRegion,
    WlOutput,
}
impl WaylandObject {
    fn as_str(&self) -> &'static str {
        match self {
            WaylandObject::WlDisplay => "wl_display",
            WaylandObject::WlRegistry => "wl_registry",
            WaylandObject::WlCallback => "wl_callback",
            WaylandObject::XdgWmBase => "xdg_wm_base",
            WaylandObject::WlShmPool(_, _) => "wl_shm_pool",
            WaylandObject::WlShm => "wl_shm",
            WaylandObject::WlBuffer(_) => "wl_buffer",
            WaylandObject::WlCompositor => "wl_compositor",
            WaylandObject::WlSurface(_) => "wl_surface",
            WaylandObject::WlRegion => "wl_region",
            WaylandObject::WlOutput => "wl_output",
        }
    }
}
impl Display for WaylandObject {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
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
                WaylandObject::WlDisplay => {
                    self.handle_wl_display_message(op_code, arg_bytes, global_state)
                        .await?;
                }
                WaylandObject::WlRegistry => {
                    self.handle_wl_registry_message(op_code, arg_bytes, global_state)
                        .await?
                }
                WaylandObject::WlCallback => self.handle_wl_callback_message(op_code).await?,
                WaylandObject::WlShm => {
                    self.handle_wl_shm_message(object_id, op_code, arg_bytes, fds)
                        .await?
                }
                WaylandObject::WlShmPool(_mmap, _fd) => {
                    self.handle_wl_shm_pool_message(object_id, op_code, arg_bytes)
                        .await?
                }
                WaylandObject::WlBuffer(_buffer_data) => {
                    self.handle_wl_buffer_message(object_id, op_code).await?
                }

                WaylandObject::WlCompositor => {
                    self.handle_wl_compositor_message(op_code, arg_bytes)
                        .await?
                }

                WaylandObject::WlSurface(_surface) => {
                    self.handle_wl_surface_message(object_id, op_code, arg_bytes)
                        .await?
                }

                WaylandObject::WlRegion => {
                    self.handle_wl_region_message(object_id, op_code, arg_bytes)
                        .await?
                }

                WaylandObject::XdgWmBase => {
                    self.handle_xdg_wm_base_message(object_id, op_code, arg_bytes)
                        .await?
                }

                WaylandObject::WlOutput => {
                    self.handle_wl_output_message(object_id, op_code, arg_bytes)
                        .await?
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

                match result {
                    Ok((0, 0)) => {
                        warn!("Connection closed while reading");
                        return;
                    }
                    Ok((data_read, fds_read)) => {
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
