use crate::api::http::{user_me, UserMe};
use crate::db::{self, Database};
use crate::fetch::feed::RSSChannel;
use crate::network_frame::KookEventMessage;
use crate::network_runtime::BotNetworkEvent;
use crate::utils::Throttle;
use crate::{api, fetch, utils, worker};
use futures_util::FutureExt;
use futures_util::StreamExt;
use log::*;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::cmp::{self};
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use thiserror::Error;
use tokio::sync::{broadcast, Notify};
use tokio_util::time::DelayQueue;

const COMMAND_RSS: &str = "/rss";
const COMMAND_SUB: &str = "/sub";
const COMMAND_UNSUB: &str = "/unsub";
const SPACE: &str = " ";
const FEED_REFRESH_INTERVAL: u32 = 2 * 60;

#[derive(Error, Debug)]
pub enum KsbotError {
    #[error("ksbot 运行时错误: {0}")]
    Runtime(String),
    #[error("ksbot 运行时内部错误: {0}")]
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
            db: Lazy::new(|| Arc::new(Database::new(None))),
        }
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
                    let feed = feed.expect("unreachable");
                    let opportunity = throttle.acquire();
                    let db = self.db.clone();
                    tokio::spawn(async move {
                        opportunity.wait().await;
                        if let Err(e) = worker::pull_feed_and_push_update(db, feed).await {
                            error!("{}", e);
                        }
                    });
                }

                _ = feed_interval.tick() => {
                    let feeds = self.db.feed_list()?;
                    for feed in feeds {
                    let feed_interval = cmp::min(
                            feed.ttl.map(|ttl| ttl * 60).unwrap_or_default(),
                            FEED_REFRESH_INTERVAL,
                    ) as u64 - 1;
                    queue.enqueue(feed, Duration::from_secs(feed_interval));
                }
                },

                net_event = net_rece.recv() => {
                    match net_event  {
                        Ok(BotNetworkEvent::Connect()) => self.on_connect().await?,
                        Ok(BotNetworkEvent::Message(msg)) => self.on_message(msg).await?,
                        Ok(BotNetworkEvent::Heart()) => self.on_pong().await?,
                        Ok(BotNetworkEvent::Error()) => self.on_error()?,
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

    fn help(&self) -> String {
        "/rss        - 显示当前订阅的 RSS 列表
/sub        - 订阅一个 RSS: /sub http://example.com/feed.xml
/unsub      - 退订一个 RSS: /unsub http://example.com/feed.xml"
            .to_owned()
    }

    // 订阅
    async fn command_sub(&self, channel: &str, url: &str) -> Result<(), KsbotError> {
        let rss = fetch::pull_feed(url).await?;
        info!("checked {}", url);
        let feed = Feed::from(&rss);
        self.db.channel_subscribed(channel, feed)?;
        Ok(())
    }

    // 取消订阅
    async fn command_unsub(&self, channel: &str, url: &str) -> Result<(), KsbotError> {
        self.db.channel_unsubscribed(channel, url)?;
        self.db.try_remove_feed(url)?;
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
            api::http::message_create(
                reply,
                channel_id.to_owned(),
                None,
                Some(msg.msg_id.unwrap()),
            )
            .await?;
        }

        Ok(())
    }

    fn on_error(&self) -> Result<(), KsbotError> {
        Ok(())
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Feed {
    pub link: String,
    pub title: String,
    pub down_time: Option<SystemTime>,
    pub ttl: Option<u32>,
    pub posts_hash: Vec<String>,
}

impl Feed {
    pub fn from(rss: &RSSChannel) -> Self {
        let posts_hash = rss
            .posts
            .iter()
            .map(|t| utils::hash(&t.link.as_ref().unwrap()))
            .collect();

        Self {
            title: rss.title.to_owned(),
            link: rss.url.to_owned(),
            down_time: Some(SystemTime::now()),
            ttl: rss.ttl,
            posts_hash,
        }
    }

    // 返回对比的两组feed, post_hash 不同的文章哈希
    // result.0 调用方 result.1 是参数方
    pub fn diff_post_index(&self, feed: &Feed) -> (Vec<usize>, Vec<usize>) {
        let ph_1 = &self.posts_hash;
        let ph_2 = &feed.posts_hash;
        let ph_1_diff = ph_1
            .iter()
            .enumerate()
            .filter(|&t| ph_2.contains(t.1))
            .map(|t| t.0)
            .collect();

        let ph_2_diff = ph_2
            .iter()
            .enumerate()
            .filter(|&t| ph_1.contains(t.1))
            .map(|t| t.0)
            .collect();

        (ph_1_diff, ph_2_diff)
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
        let exists = self.feeds.contains_key(&feed.link);
        if !exists {
            self.notifies.insert(feed.link.clone(), delay);
            self.feeds.insert(feed.link.clone(), feed);
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
