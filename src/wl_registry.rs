#![allow(dead_code)]

use crate::{
    CompositorClientState, CompositorGlobalState, WaylandObject, utils::get_wayland_string_bytes,
    wl_shm::WlShmFormat,
};
use futures::lock::MutexGuard;
use tracing::{debug, warn};

impl<'a> CompositorClientState<'a> {
    pub async fn handle_wl_registry_message(
        &mut self,
        op_code: u16,
        arg_bytes: &[u8],
        global_state: MutexGuard<'_, CompositorGlobalState>,
    ) -> anyhow::Result<()> {
        match op_code {
            0 => {
                self.handle_wl_registry_bind(arg_bytes, global_state)
                    .await?
            }
            _ => {
                warn!("Unknown op_code {} for wl_registry", op_code);
            }
        }
        Ok(())
    }

    pub async fn handle_wl_registry_bind(
        &mut self,
        arg_bytes: &[u8],
        global_state: MutexGuard<'_, CompositorGlobalState>,
    ) -> anyhow::Result<()> {
        let name = u32::from_le_bytes(arg_bytes[0..4].try_into().unwrap());
        let iface_len = u32::from_le_bytes(arg_bytes[4..8].try_into().unwrap()) as usize;
        let padded_len = (iface_len + 3) & !3;
        let interface = String::from_utf8(arg_bytes[8..8 + iface_len - 1].to_vec()).unwrap();

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
                WaylandObject::WlShm => WaylandObject::WlShm,
                WaylandObject::XdgWmBase => WaylandObject::XdgWmBase,
                WaylandObject::WlCompositor => WaylandObject::WlCompositor,
                _ => {
                    anyhow::bail!("Unknown interface requested from globals: {}", interface);
                }
            };

            if let WaylandObject::WlShm = object {
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
        Ok(())
    }

    pub async fn send_global(
        &mut self,
        registry_id: u32,
        name: u32,
        interface: &str,
        version: u32,
    ) -> anyhow::Result<()> {
        let mut args = Vec::new();
        args.extend_from_slice(&name.to_le_bytes());

        let interface_bytes = get_wayland_string_bytes(interface);
        args.extend_from_slice(&interface_bytes);
        args.extend_from_slice(&version.to_le_bytes());

        debug!(
            "Sending global {} (interface: {}, version: {}) to registry id {}",
            name, interface, version, registry_id
        );

        self.send_message(registry_id, 0, &args).await
    }
}
