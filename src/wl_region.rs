#![allow(dead_code)]

use crate::CompositorClientState;
use tracing::warn;

impl<'a> CompositorClientState<'a> {
    pub async fn handle_wl_region_message(
        &mut self,
        object_id: u32,
        op_code: u16,
        arg_bytes: &[u8],
    ) -> anyhow::Result<()> {
        warn!("Unknown op_code {} for wl_region", op_code);
        Ok(())
    }
}
