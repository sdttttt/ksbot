use crate::api::http::{user_me, UserMe};
use crate::data::Feed;
use crate::db::{self, Database};
use crate::network_frame::KookEventMessage;
use crate::network_runtime::BotNetworkEvent;
use crate::push::{push_info, push_post};
use crate::utils::{find_http_url, Throttle};
use crate::{api, fetch, push};
use futures_util::FutureExt;
use futures_util::StreamExt;
use log::*;
use once_cell::sync::Lazy;
use std::cmp;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use tokio::sync::{broadcast, Notify};
use tokio_util::time::DelayQueue;

const COMMAND_RSS: &str = "/rss";
const COMMAND_SUB: &str = "/sub";
const COMMAND_UNSUB: &str = "/unsub";
const SPACE: &str = " ";
const FEED_REFRESH_INTERVAL: u32 = 10;

#[derive(Error, Debug)]
pub enum KsbotError {
    #[error("ksbot错误: {0}")]
    Runtime(String),
    #[error("ksbot错误: {0}")]
    InnerRuntime(#[from] anyhow::Error),
    #[error("订阅错误: {0}")]
    Feed(#[from] fetch::FeedError),
    #[error("平台消息错误: {0}")]
    KookMessage(String),
    #[error("数据库错误: {0}")]
    Database(#[from] db::DatabaseError),
}

pub struct KsbotRuntime {
    me_info: Option<UserMe>,
    db: Lazy<Arc<Database>>,
}

impl KsbotRuntime {
    pub fn new() -> Self {
        Self {
            me_info: None,
            db: Lazy::new(|| Arc::new(Database::from_path(None))),
        }
    }

    fn help(&self) -> String {
        "/rss        - 显示当前订阅的 RSS 列表
/sub        - 订阅一个 RSS: /sub http://example.com/feed.xml
/unsub      - 退订一个 RSS: /unsub http://example.com/feed.xml"
            .to_owned()
    }

    // 订阅
    async fn command_sub(
        &self,
        msg: &KookEventMessage,
        subscribe_url: &str,
    ) -> Result<(), KsbotError> {
        let channel = msg.target_id.to_owned().unwrap();
        let rss = fetch::pull_feed(subscribe_url).await?;
        info!("{} 订阅了 {}", channel, subscribe_url);
        let feed = Feed::from(subscribe_url, &rss);
        self.db.channel_subscribed(&*channel, feed)?;
        push_info(&*format!("已订阅: {}", subscribe_url), msg).await?;
        push_post(&channel, &rss.posts[0]).await?;
        Ok(())
    }

    // 取消订阅
    async fn command_unsub(
        &self,
        msg: &KookEventMessage,
        subscribe_url: &str,
    ) -> Result<(), KsbotError> {
        let channel = msg.target_id.to_owned().unwrap();
        self.db.channel_unsubscribed(&*channel, subscribe_url)?;
        self.db.try_remove_feed(subscribe_url)?;
        push_info(&*format!("已取消订阅: {}", subscribe_url), msg).await?;
        Ok(())
    }

    pub async fn subscribe(
        &mut self,
        mut net_rece: broadcast::Receiver<BotNetworkEvent>,
    ) -> Result<(), KsbotError> {
        let mut feed_interval =
            tokio::time::interval(Duration::from_secs(FEED_REFRESH_INTERVAL as u64));
        let mut queue = FetchQueue::new();
        let throttle = Throttle::new(FEED_REFRESH_INTERVAL as usize, None);

        loop {
            tokio::select! {
                feed = queue.next().fuse() => {
                    let ifeed = feed.expect("unreachable");
                    info!("pull feed: {:?}", ifeed);
                    let opportunity = throttle.acquire();
                    let db = self.db.clone();
                    tokio::spawn(async move {
                        opportunity.wait().await;
                        if let Err(e) = push::push_update(db, ifeed).await {
                            error!("{}", e);
                        }
                    });
                }

                _ = feed_interval.tick() => {
                    info!("feed interval tick..");
                    let feeds = self.db.feed_list()?;
                    for feed in feeds {
                        let feed_interval = cmp::max(
                            feed.ttl.map(|ttl| ttl * 60).unwrap_or_default(),
                            FEED_REFRESH_INTERVAL,
                        ) as u64;
                        queue.enqueue(feed, Duration::from_secs(feed_interval));
                    }
                },

                net_event = net_rece.recv() => {
                    match net_event  {
                        Ok(BotNetworkEvent::Connect()) => self.on_connect().await?,
                        Ok(BotNetworkEvent::Message(ref msg)) => {
                            if let Err(e) = self.on_message(msg).await {
                                let chan_id = msg.target_id.to_owned().unwrap();
                                 let quote = msg.msg_id.to_owned().unwrap();
                                push::push_error(e, chan_id, Some(quote)).await?;
                            }
                        },
                        Ok(BotNetworkEvent::Heart()) => self.on_pong().await?,
                        Ok(BotNetworkEvent::Error()) => {},
                        Ok(BotNetworkEvent::Shutdown()) => {
                            info!("Shutting down.");
                            break;
                        },
                        Err(e) => error!("网络事件接收错误: {:?}", e),
                    }
                }
            }
        }

        Ok(())
    }

    async fn on_connect(&mut self) -> Result<(), KsbotError> {
        let me = user_me().await?;
        self.me_info = Some(me);
        Ok(())
    }

    async fn on_pong(&self) -> Result<(), KsbotError> {
        Ok(())
    }

    async fn on_message(&self, msg: &KookEventMessage) -> Result<(), KsbotError> {
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

        let content = msg.content.to_owned().unwrap_or_else(|| "".to_owned());
        let channel_id = msg.target_id.to_owned().unwrap_or_else(|| "".to_owned());
        let mut reply = Default::default();

        // 参数命令
        {
            if content.starts_with(COMMAND_SUB) {
                let cmd_args = content.split(SPACE).collect::<Vec<&str>>();
                if cmd_args.len() == 2 && !channel_id.is_empty() {
                    let url = find_http_url(cmd_args[1].trim());
                    if let Some(u) = url {
                        self.command_sub(msg, &*u).await?;
                    } else {
                        return Err(KsbotError::Runtime(format!("错误的命令：{0}", content)));
                    }
                }
            }

            if content.starts_with(COMMAND_UNSUB) {
                let cmd_args = content.split(SPACE).collect::<Vec<&str>>();
                if cmd_args.len() == 2 && !channel_id.is_empty() {
                    let url = find_http_url(cmd_args[1].trim());
                    if let Some(u) = url {
                        self.command_unsub(msg, &*u).await?;
                    } else {
                        return Err(KsbotError::Runtime(format!("错误的命令：{0}", content)));
                    }
                }
            }
        }

        // 无参数命令
        {
            match content.trim() {
                COMMAND_RSS => {
                    let feeds = self.db.channel_feed_list(&*channel_id)?;
                    if feeds.is_empty() {
                        reply = "当前没有任何订阅, 是因为太年轻犯下的错么。".to_owned()
                    } else {
                        let show_feeds = feeds
                            .iter()
                            .map(|s| format!("- [{}] {}", s.title, s.subscribe_url))
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
            api::http::message_create(
                reply,
                channel_id.to_owned(),
                None,
                Some(msg.msg_id.clone().unwrap()),
            )
            .await?;
        }

        Ok(())
    }
}

#[derive(Default)]
struct FetchQueue {
    feeds: HashMap<String, Feed>,
    notifies: DelayQueue<String>,
    wakeup: Notify,
}

impl FetchQueue {
    fn new() -> Self {
        Self::default()
    }

    fn enqueue(&mut self, feed: Feed, delay: Duration) -> bool {
        let exists = self.feeds.contains_key(&feed.subscribe_url);
        if !exists {
            self.notifies.insert(feed.subscribe_url.clone(), delay);
            self.feeds.insert(feed.subscribe_url.clone(), feed);
            self.wakeup.notify_waiters();
        }
        !exists
    }

    async fn next(&mut self) -> Result<Feed, tokio::time::error::Error> {
        loop {
            if let Some(feed_id) = self.notifies.next().await {
                let feed = self.feeds.remove(feed_id.get_ref()).unwrap();
                break Ok(feed);
            } else {
                self.wakeup.notified().await;
            }
        }
    }
}
