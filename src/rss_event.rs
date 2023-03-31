use crate::api::http::UserMe;
use crate::db::{self, Database};
use crate::fetch;
use crate::runtime_event::BotEventHook;
use crate::{api::http::KookHttpClient, ws::KookEventMessage};
use async_trait::async_trait;
use log::*;
use once_cell::sync::Lazy;
use std::sync::Arc;
use thiserror::Error;

const COMMAND_RSS: &str = "/rss";
const COMMAND_SUB: &str = "/sub";
const COMMAND_UNSUB: &str = "/unsub";
const SPACE: &str = " ";

#[derive(Error, Debug)]
pub enum KsbotError {
    #[error("订阅错误: {0}")]
    Feed(#[from] fetch::FeedError),
    #[error("平台消息错误: {0}")]
    KookMessage(String),
    #[error("数据库错误: {0}")]
    Database(#[from] db::DatabaseError),
}

pub struct KsbotRuntime {
    http_client: Option<Arc<KookHttpClient>>,
    me_info: Option<UserMe>,
    db: Lazy<Database>,
}

impl KsbotRuntime {
    pub fn new() -> Self {
        Self {
            http_client: None,
            me_info: None,
            db: Lazy::new(|| Database::new(None)),
        }
    }

    fn help(&self) -> String {
        "/rss        - 显示当前订阅的 RSS 列表
/sub        - 订阅一个 RSS: /sub http://example.com/feed.xml
/unsub      - 退订一个 RSS: /unsub http://example.com/feed.xml"
            .to_owned()
    }

    // 订阅
    async fn command_sub(&self, channel: &str, url: &str) -> Result<(), KsbotError> {
        fetch::pull_feed(url).await?;
        info!("checked {}", url);
        self.db.channel_subscribed(channel, url)?;
        Ok(())
    }

    // 取消订阅
    async fn command_unsub(&self, channel: &str, url: &str) -> Result<(), KsbotError> {
        self.db.channel_unsubscribed(channel, url)?;
        if self.db.try_remove_feed(url)? {
            todo!("对正在执行该订阅源拉取的线程进行关闭")
        }
        Ok(())
    }
}

#[async_trait]
impl BotEventHook for KsbotRuntime {
    async fn on_work(&mut self, http_client: Arc<KookHttpClient>) -> Result<(), anyhow::Error> {
        let me = http_client.user_me().await?;
        info!("{:?}", me);
        self.me_info = Some(me);
        self.http_client = Some(http_client);
        Ok(())
    }

    async fn on_pong(&self) -> Result<(), anyhow::Error> {
        Ok(())
    }

    async fn on_message(&self, msg: KookEventMessage) -> Result<(), anyhow::Error> {
        if let Some(bot) = msg.bot {
            if bot {
                info!("Bot消息，忽略...");
                return Ok(());
            }
        }

        info!(
            "channel_name = {:?}, nickname = {:?}, content = {:?}, type = {:?}, msg_timestrap = {:?} ",
            msg.channel_name, msg.nickname, msg.content, msg.typ, msg.msg_timestamp
        );

        let client = self.http_client.as_ref().unwrap();
        let content = &*msg.content.unwrap_or_else(|| "".to_owned());
        let channel_id = &*msg.target_id.unwrap_or_else(|| "".to_owned());
        let mut reply = Default::default();

        // 参数命令
        {
            if content.starts_with(COMMAND_SUB) {
                let cmd_args = content.split(SPACE).collect::<Vec<&str>>();
                if cmd_args.len() == 2 && !channel_id.is_empty() {
                    self.command_sub(channel_id, cmd_args[1]).await?;
                }
            }

            if content.starts_with(COMMAND_UNSUB) {
                let cmd_args = content.split(SPACE).collect::<Vec<&str>>();
                if cmd_args.len() == 2 && !channel_id.is_empty() {
                    self.command_unsub(channel_id, cmd_args[1]).await?;
                }
            }
        }

        // 无参数命令
        {
            match content.trim() {
                COMMAND_RSS => {
                    let feeds = self.db.channel_feed_list(channel_id)?;
                    if feeds.is_empty() {
                        reply = "当前没有任何订阅, 是因为太年轻犯下的错么。".to_owned()
                    } else {
                        let show_feeds = feeds
                            .iter()
                            .map(|s| format!("- {}", s))
                            .collect::<Vec<String>>();
                        reply = show_feeds.join("\n");
                    }
                }
                _ => {}
            }
        }

        // 帮助说明，如果被@
        {
            // `@用户名` 这种信息在content中显示的格式是：`(met){用户ID}(met)` 这样的形式
            let met_me = format!("(met){}", self.me_info.as_ref().unwrap().id);
            if content.starts_with(&met_me) {
                reply = self.help();
            }
        }
        if !reply.is_empty() {
            client
                .message_create(
                    reply,
                    channel_id.to_owned(),
                    None,
                    Some(msg.msg_id.unwrap()),
                )
                .await?;
        }

        Ok(())
    }
}
