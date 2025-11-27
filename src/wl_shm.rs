#![allow(dead_code)]

use crate::{CompositorClientState, WaylandObject};
use futures::lock::Mutex;
use memmap2::MmapOptions;
use std::{collections::VecDeque, sync::Arc};
use tracing::{debug, warn};

#[derive(Default, Clone, Copy)]
#[repr(u32)]
pub enum WlShmFormat {
    #[default]
    Argb8888 = 0,
    Rgb888 = 0x34324752,
}

impl<'a> CompositorClientState<'a> {
    pub async fn handle_wl_shm_message(
        &mut self,
        object_id: u32,
        op_code: u16,
        arg_bytes: &[u8],
        fds: &mut VecDeque<i32>,
    ) -> anyhow::Result<()> {
        match op_code {
            0 => self.handle_wl_shm_create_pool(arg_bytes, fds).await?,
            // wl_shm.release()
            1 => self.handle_wl_shm_release(object_id).await?,
            _ => {
                warn!("Unknown op_code {} for wl_shm", op_code);
            }
        }
        Ok(())
    }

    pub async fn handle_wl_shm_create_pool(
        &mut self,
        arg_bytes: &[u8],
        fds: &mut VecDeque<i32>,
    ) -> anyhow::Result<()> {
        debug!("Shm.create_pool called");
        let new_id = u32::from_le_bytes(arg_bytes[0..4].try_into().unwrap());
        let size = i32::from_le_bytes(arg_bytes[4..8].try_into().unwrap());
        let fd = fds.pop_front();

        if let Some(fd) = fd {
            // mmap size bytes of the passed in fd
            let mmap = unsafe { MmapOptions::new().len(size as usize).map_mut(fd)? };
            self.object_registry.insert(
                new_id,
                WaylandObject::WlShmPool(Arc::new(Mutex::new(mmap)), fd),
            );
        } else {
            anyhow::bail!("No file descriptor provided for shm pool creation");
        }
        Ok(())
    }

    pub async fn handle_wl_shm_release(&mut self, object_id: u32) -> anyhow::Result<()> {
        debug!("Shm.release called for id {}", object_id);
        self.object_registry.remove(&object_id);
        Ok(())
    }

    pub async fn send_format(&mut self, shm_id: u32, format: u32) -> anyhow::Result<()> {
        let argument_bytes = format.to_le_bytes();
        debug!("Sending shm format event for id {}", shm_id);
        self.send_message(shm_id, 0, &argument_bytes).await
    }
}
