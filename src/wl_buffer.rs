#![allow(dead_code)]

use crate::CompositorClientState;
use futures::lock::Mutex;
use memmap2::MmapMut;
use std::sync::Arc;
use tracing::{debug, warn};

pub struct BufferState {
    pub offset: i32,
    pub width: i32,
    pub height: i32,
    pub stride: i32,
    pub format: u32,
    pub shm_pool: Arc<Mutex<MmapMut>>,
}

impl<'a> CompositorClientState<'a> {
    pub async fn handle_wl_buffer_message(
        &mut self,
        object_id: u32,
        op_code: u16,
    ) -> anyhow::Result<()> {
        match op_code {
            0 => {
                self.handle_wl_buffer_destroy(object_id).await?;
            }
            _ => {
                warn!("Unknown op_code {} for wl_buffer", op_code);
            }
        }
        Ok(())
    }

    pub async fn handle_wl_buffer_destroy(&mut self, object_id: u32) -> anyhow::Result<()> {
        debug!("Buffer.destroy called for id {}", object_id);
        self.object_registry.remove(&object_id);
        Ok(())
    }

    pub async fn send_wl_buffer_release(&mut self, object_id: u32) -> anyhow::Result<()> {
        self.send_message(object_id, 0, &[]).await
    }
}
