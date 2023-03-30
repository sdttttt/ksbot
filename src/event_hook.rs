use crate::{api::http::KookHttpClient, ws::KookEventMessage};
use async_trait::async_trait;
use std::sync::Arc;

#[async_trait]
pub trait BotEventHook {
    // 准备好了,保存一下这个http_client.
    fn on_ready(&mut self, http_client: Arc<KookHttpClient>) -> Result<(), anyhow::Error>;
    async fn on_pong(&self) -> Result<(), anyhow::Error>;
    async fn on_message(&self, msg: KookEventMessage) -> Result<(), anyhow::Error>;
}
