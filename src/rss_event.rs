use crate::api::http::UserMe;
use crate::event_hook::BotEventHook;
use crate::{api::http::KookHttpClient, ws::KookEventMessage};
use anyhow::Ok;
use async_trait::async_trait;
use std::sync::Arc;

pub struct KsbotRuntime {
    http_client: Option<Arc<KookHttpClient>>,
    me_info: Option<UserMe>,
}

impl KsbotRuntime {
    pub fn new() -> Self {
        Self {
            http_client: None,
            me_info: None,
        }
    }

    pub fn help(&self) -> String {
        "/rss        - 显示当前订阅的 RSS 列表
/sub        - 订阅一个 RSS: /sub http://example.com/feed.xml
/unsub      - 退订一个 RSS: /unsub http://example.com/feed.xml"
            .to_owned()
    }
}

#[async_trait]
impl BotEventHook for KsbotRuntime {
    async fn on_work(&mut self, http_client: Arc<KookHttpClient>) -> Result<(), anyhow::Error> {
        let me = http_client.user_me().await?;
        println!("{:?}", me);
        self.me_info = Some(me);
        self.http_client = Some(http_client);
        Ok(())
    }

    async fn on_pong(&self) -> Result<(), anyhow::Error> {
        Ok(())
    }

    async fn on_message(&self, msg: KookEventMessage) -> Result<(), anyhow::Error> {
        let client = self.http_client.as_ref().unwrap();
        let content = &*msg.content.unwrap_or_else(|| "".to_owned());
        let mut reply = Default::default();

        match content.trim() {
            "/rss" => reply = "显示当前订阅的 RSS 列表".to_owned(),
            _ => {}
        }

        // `@用户名` 这种信息在content中显示的格式是：`(met){用户ID}(met)` 这样的形式
        let met_me = format!("(met){}", self.me_info.as_ref().unwrap().id);
        if content.starts_with(&met_me) {
            reply = self.help();
        }

        if !reply.is_empty() {
            client
                .message_create(
                    reply,
                    msg.target_id.unwrap(),
                    None,
                    Some(msg.msg_id.unwrap()),
                )
                .await?;
        }

        Ok(())
    }
}
