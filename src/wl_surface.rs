#![allow(dead_code)]

use crate::{CompositorClientState, WaylandObject, wl_output::WlOutputTransform};
use tracing::{debug, warn};

#[derive(Default)]
pub struct SurfaceState {
    pending_buffer: Option<u32>,
    current_buffer: Option<u32>,
    pending_surface_damage: Vec<(i32, i32, i32, i32)>,
    current_surface_damage: Vec<(i32, i32, i32, i32)>,
    pending_buffer_damage: Vec<(i32, i32, i32, i32)>,
    current_buffer_damage: Vec<(i32, i32, i32, i32)>,
    pending_opaque_region: Option<u32>,
    current_opaque_region: Option<u32>,
    pending_input_region: Option<u32>,
    current_input_region: Option<u32>,
    pending_transform: WlOutputTransform,
    current_transform: WlOutputTransform,
    pending_scale: i32,
    current_scale: i32,
    pending_offset: (i32, i32),
    current_offset: (i32, i32),
    frame_callbacks: Vec<u32>,
}

impl<'a> CompositorClientState<'a> {
    pub async fn handle_wl_surface_message(
        &mut self,
        object_id: u32,
        op_code: u16,
        arg_bytes: &[u8],
    ) -> anyhow::Result<()> {
        match op_code {
            0 => self.handle_wl_surface_destroy(object_id).await?,
            1 => self.handle_wl_surface_attach(object_id, arg_bytes).await?,
            2 => self.handle_wl_surface_damage(object_id, arg_bytes).await?,
            3 => self.handle_wl_surface_frame(object_id, arg_bytes).await?,
            4 => {
                self.handle_wl_surface_set_opaque_region(object_id, arg_bytes)
                    .await?
            }
            5 => {
                self.handle_wl_surface_set_input_region(object_id, arg_bytes)
                    .await?
            }
            6 => {
                self.handle_wl_surface_commit(object_id).await?;
            }
            7 => {
                self.handle_wl_surface_set_buffer_transform(object_id, arg_bytes)
                    .await?
            }
            8 => {
                self.handle_wl_surface_set_buffer_scale(object_id, arg_bytes)
                    .await?
            }
            9 => {
                self.handle_wl_surface_damage_buffer(object_id, arg_bytes)
                    .await?
            }
            // wl_surface.offset(x:int, y:int)
            10 => self.handle_wl_surface_offset(object_id, arg_bytes).await?,

            _ => {
                warn!("Unknown op_code {} for wl_surface", op_code);
            }
        }
        Ok(())
    }

    pub async fn handle_wl_surface_destroy(&mut self, object_id: u32) -> anyhow::Result<()> {
        debug!("WlSurface.destroy called for id {}", object_id);
        self.object_registry.remove(&object_id);
        Ok(())
    }

    pub async fn handle_wl_surface_attach(
        &mut self,
        object_id: u32,
        arg_bytes: &[u8],
    ) -> anyhow::Result<()> {
        let buffer_id = u32::from_le_bytes(arg_bytes[0..4].try_into().unwrap());
        let x = i32::from_le_bytes(arg_bytes[4..8].try_into().unwrap());
        let y = i32::from_le_bytes(arg_bytes[8..12].try_into().unwrap());

        let surface_object = self
            .object_registry
            .get_mut(&object_id)
            .ok_or_else(|| anyhow::anyhow!("WlSurface object not found for id {}", object_id))?;
        let surface = match surface_object {
            WaylandObject::WlSurface(surface) => surface,
            _ => {
                anyhow::bail!("Object id {} is not a WlSurface", object_id);
            }
        };

        debug!(
            "WlSurface.attach called with buffer_id {}, x {}, y {}",
            buffer_id, x, y
        );
        surface.pending_buffer = Some(buffer_id);
        Ok(())
    }

    pub async fn handle_wl_surface_damage(
        &mut self,
        object_id: u32,
        arg_bytes: &[u8],
    ) -> anyhow::Result<()> {
        let x = i32::from_le_bytes(arg_bytes[0..4].try_into().unwrap());
        let y = i32::from_le_bytes(arg_bytes[4..8].try_into().unwrap());
        let width = i32::from_le_bytes(arg_bytes[8..12].try_into().unwrap());
        let height = i32::from_le_bytes(arg_bytes[12..16].try_into().unwrap());

        let surface_object = self
            .object_registry
            .get_mut(&object_id)
            .ok_or_else(|| anyhow::anyhow!("WlSurface object not found for id {}", object_id))?;
        let surface = match surface_object {
            WaylandObject::WlSurface(surface) => surface,
            _ => {
                anyhow::bail!("Object id {} is not a WlSurface", object_id);
            }
        };

        debug!(
            "WlSurface.damage called with x {}, y {}, width {}, height {}",
            x, y, width, height
        );
        surface.pending_surface_damage.push((x, y, width, height));
        Ok(())
    }

    pub async fn handle_wl_surface_frame(
        &mut self,
        object_id: u32,
        arg_bytes: &[u8],
    ) -> anyhow::Result<()> {
        let new_id = u32::from_le_bytes(arg_bytes[0..4].try_into().unwrap());
        let surface_object = self
            .object_registry
            .get_mut(&object_id)
            .ok_or_else(|| anyhow::anyhow!("WlSurface object not found for id {}", object_id))?;
        let surface = match surface_object {
            WaylandObject::WlSurface(surface) => surface,
            _ => {
                anyhow::bail!("Object id {} is not a WlSurface", object_id);
            }
        };

        debug!("WlSurface.frame called with new_id {}", new_id);
        surface.frame_callbacks.push(new_id);
        self.object_registry.insert(new_id, WaylandObject::Callback);
        Ok(())
    }

    pub async fn handle_wl_surface_set_opaque_region(
        &mut self,
        object_id: u32,
        arg_bytes: &[u8],
    ) -> anyhow::Result<()> {
        let region_id = u32::from_le_bytes(arg_bytes[..4].try_into().unwrap());
        let surface_object = self
            .object_registry
            .get_mut(&object_id)
            .ok_or_else(|| anyhow::anyhow!("WlSurface object not found for id {}", object_id))?;
        let surface = match surface_object {
            WaylandObject::WlSurface(surface) => surface,
            _ => {
                anyhow::bail!("Object id {} is not a WlSurface", object_id);
            }
        };

        debug!(
            "WlSurface.set_opaque_region called with region_id {}",
            region_id
        );
        surface.pending_opaque_region = Some(region_id);
        Ok(())
    }

    pub async fn handle_wl_surface_set_input_region(
        &mut self,
        object_id: u32,
        arg_bytes: &[u8],
    ) -> anyhow::Result<()> {
        let region_id = u32::from_le_bytes(arg_bytes[..4].try_into().unwrap());
        let surface_object = self
            .object_registry
            .get_mut(&object_id)
            .ok_or_else(|| anyhow::anyhow!("WlSurface object not found for id {}", object_id))?;
        let surface = match surface_object {
            WaylandObject::WlSurface(surface) => surface,
            _ => {
                anyhow::bail!("Object id {} is not a WlSurface", object_id);
            }
        };

        debug!(
            "WlSurface.set_input_region called with region_id {}",
            region_id
        );
        surface.pending_input_region = Some(region_id);
        Ok(())
    }

    pub async fn handle_wl_surface_commit(&mut self, object_id: u32) -> anyhow::Result<()> {
        let surface_object = self
            .object_registry
            .get_mut(&object_id)
            .ok_or_else(|| anyhow::anyhow!("WlSurface object not found for id {}", object_id))?;
        let surface = match surface_object {
            WaylandObject::WlSurface(surface) => surface,
            _ => {
                anyhow::bail!("Object id {} is not a WlSurface", object_id);
            }
        };

        debug!("WlSurface.commit called");
        surface.current_buffer = surface.pending_buffer.take();
        surface.current_surface_damage = std::mem::take(&mut surface.pending_surface_damage);
        surface.current_buffer_damage = std::mem::take(&mut surface.pending_buffer_damage);
        surface.current_opaque_region = surface.pending_opaque_region.take();
        surface.current_input_region = surface.pending_input_region.take();
        surface.current_transform = surface.pending_transform;
        surface.current_scale = surface.pending_scale;
        surface.current_offset = surface.pending_offset;
        // TODO: Rendering the surface would happen here
        // TODO: Maybe release the buffer?

        let callback_ids = surface.frame_callbacks.drain(..).collect::<Vec<u32>>();
        for callback_id in callback_ids {
            self.send_callback_done(callback_id, 0).await?;
        }

        Ok(())
    }

    pub async fn handle_wl_surface_set_buffer_transform(
        &mut self,
        object_id: u32,
        arg_bytes: &[u8],
    ) -> anyhow::Result<()> {
        let transform = i32::from_le_bytes(arg_bytes[..4].try_into().unwrap());
        let surface_object = self
            .object_registry
            .get_mut(&object_id)
            .ok_or_else(|| anyhow::anyhow!("WlSurface object not found for id {}", object_id))?;
        let surface = match surface_object {
            WaylandObject::WlSurface(surface) => surface,
            _ => {
                anyhow::bail!("Object id {} is not a WlSurface", object_id);
            }
        };

        debug!(
            "WlSurface.set_buffer_transform called with transform {}",
            transform
        );
        surface.pending_transform =
            unsafe { std::mem::transmute::<i32, WlOutputTransform>(transform) };
        Ok(())
    }

    pub async fn handle_wl_surface_set_buffer_scale(
        &mut self,
        object_id: u32,
        arg_bytes: &[u8],
    ) -> anyhow::Result<()> {
        let scale = i32::from_le_bytes(arg_bytes[..4].try_into().unwrap());
        let surface_object = self
            .object_registry
            .get_mut(&object_id)
            .ok_or_else(|| anyhow::anyhow!("WlSurface object not found for id {}", object_id))?;
        let surface = match surface_object {
            WaylandObject::WlSurface(surface) => surface,
            _ => {
                anyhow::bail!("Object id {} is not a WlSurface", object_id);
            }
        };

        debug!("WlSurface.set_buffer_scale called with scale {}", scale);
        surface.pending_scale = scale;
        Ok(())
    }

    pub async fn handle_wl_surface_damage_buffer(
        &mut self,
        object_id: u32,
        arg_bytes: &[u8],
    ) -> anyhow::Result<()> {
        let x = i32::from_le_bytes(arg_bytes[0..4].try_into().unwrap());
        let y = i32::from_le_bytes(arg_bytes[4..8].try_into().unwrap());
        let width = i32::from_le_bytes(arg_bytes[8..12].try_into().unwrap());
        let height = i32::from_le_bytes(arg_bytes[12..16].try_into().unwrap());

        let surface_object = self
            .object_registry
            .get_mut(&object_id)
            .ok_or_else(|| anyhow::anyhow!("WlSurface object not found for id {}", object_id))?;
        let surface = match surface_object {
            WaylandObject::WlSurface(surface) => surface,
            _ => {
                anyhow::bail!("Object id {} is not a WlSurface", object_id);
            }
        };

        debug!(
            "WlSurface.damage_buffer called with x {}, y {}, width {}, height {}",
            x, y, width, height
        );
        surface.pending_buffer_damage.push((x, y, width, height));
        Ok(())
    }

    pub async fn handle_wl_surface_offset(
        &mut self,
        object_id: u32,
        arg_bytes: &[u8],
    ) -> anyhow::Result<()> {
        let x = i32::from_le_bytes(arg_bytes[0..4].try_into().unwrap());
        let y = i32::from_le_bytes(arg_bytes[4..8].try_into().unwrap());
        let surface_object = self
            .object_registry
            .get_mut(&object_id)
            .ok_or_else(|| anyhow::anyhow!("WlSurface object not found for id {}", object_id))?;
        let surface = match surface_object {
            WaylandObject::WlSurface(surface) => surface,
            _ => {
                anyhow::bail!("Object id {} is not a WlSurface", object_id);
            }
        };

        debug!("WlSurface.offset called with x {}, y {}", x, y);

        surface.pending_offset = (x, y);
        Ok(())
    }
}

pub async fn send_wl_surface_enter(
    client_state: &mut CompositorClientState<'_>,
    surface_id: u32,
    output_id: u32,
) -> anyhow::Result<()> {
    client_state
        .send_message(surface_id, 0, &output_id.to_le_bytes())
        .await
}

pub async fn send_wl_surface_leave(
    client_state: &mut CompositorClientState<'_>,
    surface_id: u32,
    output_id: u32,
) -> anyhow::Result<()> {
    client_state
        .send_message(surface_id, 1, &output_id.to_le_bytes())
        .await
}

pub async fn send_wl_surface_preferred_buffer_scale(
    client_state: &mut CompositorClientState<'_>,
    surface_id: u32,
    scale: i32,
) -> anyhow::Result<()> {
    client_state
        .send_message(surface_id, 2, &scale.to_le_bytes())
        .await
}

pub async fn send_wl_surface_preferred_buffer_transform(
    client_state: &mut CompositorClientState<'_>,
    surface_id: u32,
    transform: WlOutputTransform,
) -> anyhow::Result<()> {
    let transform_int = transform as i32;
    client_state
        .send_message(surface_id, 3, &transform_int.to_le_bytes())
        .await
}
