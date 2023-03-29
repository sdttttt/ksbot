use std::sync::Arc;
use crate::{api::http::KookHttpClient, ws::KookChannelMessage};
use crate::event_hook::BotEventHook;
use async_trait::async_trait;

pub struct RSSEvent {
    http_client: Option<Arc<KookHttpClient>>,
}

impl RSSEvent {
    pub fn new() -> Self {
        Self { http_client: None }
    }
}

#[async_trait]
impl BotEventHook for RSSEvent {
    fn on_ready(&mut self, http_client: Arc<KookHttpClient>) -> Result<(), anyhow::Error> {
        self.http_client = Some(http_client);
        Ok(())
    }

    async fn on_pong(&self) -> Result<(), anyhow::Error> {
        Ok(())
    }

    async fn on_message(&self, msg: KookChannelMessage) -> Result<(), anyhow::Error> {
        if let Some(ref c) = self.http_client {
            c.message_create("啊这...".to_owned(), msg.target_id.unwrap(), None, Some(msg.msg_id.unwrap())).await?;
        }
        Ok(())
    }
}
