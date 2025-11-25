use futures::lock::Mutex;
use std::{collections::HashMap, fmt::Display, sync::Arc};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{UnixSocket, UnixStream},
};
use tracing::{debug, error, warn};

struct MyCompositor {
    object_registry: HashMap<u32, WaylandObject>,
    globals: Vec<(u32, WaylandObject, u32)>,
}
impl Default for MyCompositor {
    fn default() -> Self {
        let mut object_registry = HashMap::new();
        object_registry.insert(1, WaylandObject::Display);
        MyCompositor {
            object_registry,
            globals: vec![(1, WaylandObject::XdgWmBase, 7)],
        }
    }
}

#[derive(Clone)]
enum WaylandObject {
    Display,
    Registry,
    Callback,
    XdgWmBase,
}
impl WaylandObject {
    fn as_str(&self) -> &'static str {
        match self {
            WaylandObject::Display => "wl_display",
            WaylandObject::Registry => "wl_registry",
            WaylandObject::Callback => "wl_callback",
            WaylandObject::XdgWmBase => "xdg_wm_base",
        }
    }
}
impl Display for WaylandObject {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
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

async fn send_message(
    stream: &mut UnixStream,
    object_id: u32,
    op_code: u16,
    args: &[u8],
) -> anyhow::Result<()> {
    stream.write_all(&object_id.to_le_bytes()).await?;
    stream.write_all(&op_code.to_le_bytes()).await?;
    stream
        .write_all(&(8 + args.len() as u16).to_le_bytes())
        .await?;
    stream.write_all(args).await?;
    Ok(())
}

impl MyCompositor {
    async fn handle_message(
        &mut self,
        object_id: u32,
        op_code: u16,
        arg_bytes: &[u8],
        stream: &mut UnixStream,
    ) -> anyhow::Result<()> {
        if let Some(object) = self.object_registry.get_mut(&object_id) {
            match object {
                WaylandObject::Display => match op_code {
                    0 => {
                        let new_id = u32::from_le_bytes(arg_bytes[..4].try_into().unwrap());
                        debug!("Display sync called with new_id {}", new_id);
                        if stream.writable().await.is_err() {
                            error!("Failed to await writability on socket");
                        } else {
                            self.object_registry.insert(new_id, WaylandObject::Callback);
                            let argument_bytes = [0u8; 4];
                            debug!("Sending callback done event for id {}", new_id);
                            send_message(stream, new_id, 0, &argument_bytes).await?;
                        }
                    }
                    1 => {
                        let new_id = u32::from_le_bytes(arg_bytes[..4].try_into().unwrap());
                        debug!("Display get_registry called with new_id {}", new_id);
                        if stream.writable().await.is_err() {
                            error!("Failed to await writability on socket");
                        } else {
                            self.object_registry.insert(new_id, WaylandObject::Registry);
                            for (name, interface, version) in &self.globals {
                                let mut args = Vec::new();
                                args.extend_from_slice(&name.to_le_bytes());

                                let interface_bytes = get_string_bytes(interface.as_str()).await;
                                args.extend_from_slice(&interface_bytes);
                                args.extend_from_slice(&version.to_le_bytes());

                                debug!(
                                    "Sending global {} (interface: {}, version: {}) to registry id {}",
                                    name, interface, version, new_id
                                );

                                send_message(stream, new_id, 0, &args).await?;
                            }
                        }
                    }
                    _ => {
                        warn!("Unknown op_code {} for Display", op_code);
                    }
                },
                WaylandObject::Registry => match op_code {
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
                            self.globals.iter().find(|(n, _, _)| *n == name)
                        {
                            let object = interface.clone();
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
                    warn!("No op_codes implemented for Callback");
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

    let display_mutex = Arc::new(Mutex::new(MyCompositor::default()));

    let socket_path = "/tmp/my-wayland-socket.sock";
    let _ = std::fs::remove_file(socket_path);

    let socket = UnixSocket::new_stream()?;
    socket.bind(socket_path)?;

    let listener = socket.listen(1024)?;
    println!("Listening on {:?}", socket_path);

    loop {
        let (mut stream, _) = listener.accept().await?;
        let display_mutex = display_mutex.clone();

        tokio::spawn(async move {
            loop {
                if stream.readable().await.is_err() {
                    error!("Failed to await readability on socket");
                    return;
                }

                let mut buffer = [0u8; 8];
                match stream.read_exact(&mut buffer).await {
                    Ok(_) => {}
                    Err(e) => {
                        warn!("Connection closed or error while reading header: {}", e);
                        return;
                    }
                }

                let object_id = u32::from_le_bytes([buffer[0], buffer[1], buffer[2], buffer[3]]);
                let op_code = u16::from_le_bytes([buffer[4], buffer[5]]) as usize;
                let message_length = u16::from_le_bytes([buffer[6], buffer[7]]);

                let mut args_buffer = vec![0u8; (message_length - 8) as usize];

                match stream.read_exact(&mut args_buffer).await {
                    Ok(_) => {}
                    Err(e) => {
                        warn!(
                            "Connection closed or error while reading message body: {}",
                            e
                        );
                        return;
                    }
                }

                let mut display = display_mutex.lock().await;
                let res = display
                    .handle_message(object_id, op_code as u16, &args_buffer, &mut stream)
                    .await;

                if let Err(e) = res {
                    error!("Error handling message: {}", e);
                    error!("Closing connection due to error.");
                    stream.shutdown().await.ok();
                    return;
                }
            }
        });
    }
}
