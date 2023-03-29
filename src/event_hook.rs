use std::sync::Arc;

trait BotHook {
    // 准备好了,保存一下这个http_client.
    fn on_ready(&self, http_client: Arc<reqwest::Client>) -> Result<(), anyhow::Error>;
    fn on_pong(&self) -> Result<(), anyhow::Error>;
    fn on_message(&self) -> Result<(), anyhow::Error>;
}
