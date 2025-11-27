#![allow(dead_code)]

use crate::CompositorClientState;
use tracing::warn;

impl<'a> CompositorClientState<'a> {
    pub async fn handle_xdg_wm_base_message(
        &mut self,
        object_id: u32,
        op_code: u16,
        arg_bytes: &[u8],
    ) -> anyhow::Result<()> {
        warn!("Unknown op_code {} for xdg_wm_base", op_code);
        Ok(())
    }
}
