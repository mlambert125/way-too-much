#![allow(dead_code)]

use crate::{CompositorClientState, WaylandObject, wl_surface::SurfaceState};
use tracing::{debug, warn};

impl<'a> CompositorClientState<'a> {
    pub async fn handle_wl_compositor_message(
        &mut self,
        op_code: u16,
        arg_bytes: &[u8],
    ) -> anyhow::Result<()> {
        match op_code {
            0 => self.handle_wl_compositor_create_surface(arg_bytes).await?,
            1 => self.handle_wl_compositor_create_region(arg_bytes).await?,
            _ => {
                warn!("Unknown op_code {} for wl_compositor", op_code);
            }
        }
        Ok(())
    }

    pub async fn handle_wl_compositor_create_surface(
        &mut self,
        arg_bytes: &[u8],
    ) -> anyhow::Result<()> {
        let new_id = u32::from_le_bytes(arg_bytes[..4].try_into().unwrap());
        debug!("WlCompositor.create_surface called with new_id {}", new_id);
        self.object_registry
            .insert(new_id, WaylandObject::WlSurface(SurfaceState::default()));
        Ok(())
    }

    pub async fn handle_wl_compositor_create_region(
        &mut self,
        arg_bytes: &[u8],
    ) -> anyhow::Result<()> {
        let new_id = u32::from_le_bytes(arg_bytes[..4].try_into().unwrap());
        debug!("WlCompositor.create_region called with new_id {}", new_id);
        self.object_registry.insert(new_id, WaylandObject::WlRegion);
        Ok(())
    }
}
