#![allow(dead_code)]

use crate::{CompositorClientState, WaylandObject, wl_buffer::BufferState};
use futures::lock::Mutex;
use memmap2::{MmapMut, RemapOptions};
use std::sync::Arc;
use tracing::{debug, warn};

impl<'a> CompositorClientState<'a> {
    pub async fn handle_wl_shm_pool_message(
        &mut self,
        object_id: u32,
        op_code: u16,
        arg_bytes: &[u8],
    ) -> anyhow::Result<()> {
        let shm_pool_object = self
            .object_registry
            .get(&object_id)
            .ok_or_else(|| anyhow::anyhow!("ShmPool object not found for id {}", object_id))?;
        let mmap = match shm_pool_object {
            WaylandObject::WlShmPool(mmap, _) => mmap,
            _ => {
                anyhow::bail!("Object id {} is not a ShmPool", object_id);
            }
        };

        match op_code {
            0 => {
                self.handle_wl_shm_pool_create_buffer(arg_bytes, mmap.clone())
                    .await?
            }
            1 => self.handle_wl_shm_pool_destroy(object_id).await?,
            2 => {
                self.handle_wl_shm_pool_resize(arg_bytes, mmap.clone())
                    .await?
            }
            _ => {
                warn!("Unknown op_code {} for ShmPool", op_code);
            }
        }
        Ok(())
    }

    pub async fn handle_wl_shm_pool_create_buffer(
        &mut self,
        arg_bytes: &[u8],
        mmap: Arc<Mutex<MmapMut>>,
    ) -> anyhow::Result<()> {
        debug!("ShmPool.create_buffer called");
        let new_id = u32::from_le_bytes(arg_bytes[0..4].try_into().unwrap());
        let offset = i32::from_le_bytes(arg_bytes[4..8].try_into().unwrap());
        let width = i32::from_le_bytes(arg_bytes[8..12].try_into().unwrap());
        let height = i32::from_le_bytes(arg_bytes[12..16].try_into().unwrap());
        let stride = i32::from_le_bytes(arg_bytes[16..20].try_into().unwrap());
        let format = u32::from_le_bytes(arg_bytes[20..24].try_into().unwrap());
        let buffer = BufferState {
            offset,
            width,
            height,
            stride,
            format,
            shm_pool: mmap.clone(),
        };
        self.object_registry
            .insert(new_id, WaylandObject::WlBuffer(buffer));
        Ok(())
    }

    pub async fn handle_wl_shm_pool_destroy(&mut self, object_id: u32) -> anyhow::Result<()> {
        debug!("ShmPool.destroy called for id {}", object_id);
        self.object_registry.remove(&object_id);
        Ok(())
    }

    pub async fn handle_wl_shm_pool_resize(
        &mut self,
        arg_bytes: &[u8],
        mmap: Arc<Mutex<MmapMut>>,
    ) -> anyhow::Result<()> {
        debug!("ShmPool.resize called");
        let new_size = u32::from_le_bytes(arg_bytes[0..4].try_into().unwrap());
        let mut mmap = mmap.lock().await;
        unsafe {
            mmap.remap(new_size as usize, RemapOptions::new().may_move(false))?;
        }
        Ok(())
    }
}
