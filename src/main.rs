use futures::lock::Mutex;
use std::{collections::HashMap, sync::Arc};
use tokio::net::{UnixSocket, UnixStream};
use tokio::prelude::*;
use tracing::{debug, error, warn};

struct MyCompositor {
    object_registry: HashMap<u32, WaylandObject>,
}
impl Default for MyCompositor {
    fn default() -> Self {
        let mut object_registry = HashMap::new();
        object_registry.insert(1, WaylandObject::Display);
        object_registry.insert(2, WaylandObject::Registry);
        MyCompositor { object_registry }
    }
}

enum WaylandObject {
    Display,
    Registry,
    Callback,
}

fn get_message_bytes(object_id: u32, op_code: u16, args: &[u8], buffer: &mut [u8]) {
    let object_id_bytes = object_id.to_le_bytes();
    let op_code_bytes = op_code.to_le_bytes();
    let length_bytes = (8 + args.len() as u16).to_le_bytes();

    buffer[0..4].copy_from_slice(&object_id_bytes);
    buffer[4..6].copy_from_slice(&op_code_bytes);
    buffer[6..8].copy_from_slice(&length_bytes);
    buffer[8..(8 + args.len())].copy_from_slice(args);
}

impl MyCompositor {
    async fn handle_message(
        &mut self,
        object_id: u32,
        op_code: u16,
        message: &[u8],
        stream: &mut UnixStream,
    ) -> anyhow::Result<()> {
        if let Some(object) = self.object_registry.get_mut(&object_id) {
            match object {
                WaylandObject::Display => match op_code {
                    0 => {
                        let new_id =
                            u32::from_le_bytes([message[0], message[1], message[2], message[3]]);
                        debug!("Display sync called with new_id {}", new_id);
                        if stream.writable().await.is_err() {
                            error!("Failed to await writability on socket");
                        } else {
                            self.object_registry.insert(new_id, WaylandObject::Callback);
                            let argument_bytes = [0u8; 4];
                            let mut response = [0u8; 12];
                            get_message_bytes(new_id, 0, &argument_bytes, &mut response);

                            stream.write_all(&response).await?;
                        }
                    }
                    1 => {
                        let new_id =
                            u32::from_le_bytes([message[0], message[1], message[2], message[3]]);
                        debug!("Display get_registry called with new_id {}", new_id);
                    }
                    _ => {
                        warn!("Unknown op_code {} for Display", op_code);
                    }
                },
                WaylandObject::Registry => match op_code {
                    0 => {
                        let name =
                            u32::from_le_bytes([message[0], message[1], message[2], message[3]]);
                        let new_id =
                            u32::from_le_bytes([message[4], message[5], message[6], message[7]]);
                        debug!(
                            "Registry bind called with new_id {} and name {}",
                            new_id, name
                        );
                    }
                    _ => {
                        warn!("Unknown op_code {} for Registry", op_code);
                    }
                },
                WaylandObject::Callback => {
                    warn!("No op_codes defined for Callback");
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
                let mut total_read = 0;
                while total_read < 8 {
                    match stream.try_read(&mut buffer[total_read..]) {
                        Ok(0) => {
                            warn!("Connection closed by peer");
                            return;
                        }
                        Ok(n) => {
                            total_read += n;
                        }
                        Err(e) => {
                            error!("Failed to read from socket: {}", e);
                            return;
                        }
                    }
                }

                let object_id = u32::from_le_bytes([buffer[0], buffer[1], buffer[2], buffer[3]]);
                let op_code = u16::from_le_bytes([buffer[4], buffer[5]]) as usize;
                let message_length = u16::from_le_bytes([buffer[6], buffer[7]]);
                debug!(
                    "Object ID: {}, Message Length: {}, Op Code: {}",
                    object_id, message_length, op_code
                );

                let mut message_buffer = vec![0u8; (message_length - 8) as usize];
                let mut total_read = 0;
                while total_read < message_buffer.len() {
                    let _ = stream.readable().await;
                    match stream.try_read(&mut message_buffer[total_read..]) {
                        Ok(0) => {
                            warn!("Connection closed while reading message");
                            return;
                        }
                        Ok(n) => {
                            total_read += n;
                        }
                        Err(e) => {
                            error!("Failed to read message body: {}", e);
                            return;
                        }
                    }
                }
                debug!("Received message body: {:?}", message_buffer);

                let mut display = display_mutex.lock().await;
                display
                    .handle_message(object_id, op_code as u16, &message_buffer, &mut stream)
                    .await;
            }
        });
    }
}
