#![allow(dead_code)]

use crate::CompositorClientState;
use tracing::warn;

#[derive(Default, Clone, Copy)]
#[repr(u32)]
pub enum WlOutputTransform {
    #[default]
    Normal = 0,
    Rotate90 = 1,
    Rotate180 = 2,
    Rotate270 = 3,
    Flipped = 4,
    Flipped90 = 5,
    Flipped180 = 6,
    Flipped270 = 7,
}

impl<'a> CompositorClientState<'a> {
    pub async fn handle_wl_output_message(&mut self, op_code: u16) -> anyhow::Result<()> {
        warn!("Unknown op_code {} for wl_output", op_code);
        Ok(())
    }
}
