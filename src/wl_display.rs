#![allow(dead_code)]

use crate::{
    CompositorClientState, CompositorGlobalState, WaylandObject, utils::get_wayland_string_bytes,
};
use futures::lock::MutexGuard;
use tracing::{debug, warn};

impl<'a> CompositorClientState<'a> {
    pub async fn handle_wl_display_message(
        &mut self,
        op_code: u16,
        arg_bytes: &[u8],
        global_state: MutexGuard<'_, CompositorGlobalState>,
    ) -> anyhow::Result<()> {
        match op_code {
            0 => self.handle_wl_display_sync(arg_bytes).await?,
            1 => {
                self.handle_wl_display_get_registry(arg_bytes, global_state)
                    .await?
            }
            _ => {
                warn!("Unknown op_code {} for wl_display", op_code);
            }
        }
        Ok(())
    }

    pub async fn handle_wl_display_sync(&mut self, arg_bytes: &[u8]) -> anyhow::Result<()> {
        let new_id = u32::from_le_bytes(arg_bytes[..4].try_into().unwrap());
        debug!("Display sync called with new_id {}", new_id);

        self.object_registry
            .insert(new_id, WaylandObject::WlCallback);
        self.send_callback_done(new_id, 0).await?;
        Ok(())
    }

    pub async fn handle_wl_display_get_registry(
        &mut self,
        arg_bytes: &[u8],
        global_state: MutexGuard<'_, CompositorGlobalState>,
    ) -> anyhow::Result<()> {
        let new_id = u32::from_le_bytes(arg_bytes[..4].try_into().unwrap());
        debug!("Display get_registry called with new_id {}", new_id);
        self.object_registry
            .insert(new_id, WaylandObject::WlRegistry);

        for (name, interface, version) in &global_state.globals {
            self.send_global(new_id, *name, interface.as_str(), *version)
                .await?;
        }
        Ok(())
    }

    pub async fn send_wl_display_error(&mut self, code: u32, message: &str) -> anyhow::Result<()> {
        let mut args = Vec::new();
        args.extend_from_slice(&code.to_le_bytes());
        args.extend_from_slice(&get_wayland_string_bytes(message));

        self.send_message(1, 0, &args).await
    }

    pub async fn send_wl_display_delete_id(&mut self, id: u32) -> anyhow::Result<()> {
        let mut args = Vec::new();
        args.extend_from_slice(&id.to_le_bytes());

        self.send_message(1, 1, &args).await
    }
}
