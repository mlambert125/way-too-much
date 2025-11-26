#![allow(dead_code)]

use crate::CompositorClientState;
use tracing::{debug, warn};

impl<'a> CompositorClientState<'a> {
    pub async fn handle_wl_callback_message(&mut self, op_code: u16) -> anyhow::Result<()> {
        warn!("Unknown op_code {} for wl_callback", op_code);
        Ok(())
    }

    pub async fn send_callback_done(
        &mut self,
        callback_id: u32,
        callback_data: u32,
    ) -> anyhow::Result<()> {
        let argument_bytes = callback_data.to_le_bytes();
        debug!("Sending callback done event for id {}", callback_id);
        self.send_message(callback_id, 0, &argument_bytes).await
    }
}
