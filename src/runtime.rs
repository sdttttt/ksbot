use crate::api::http::{user_me, UserMe};
use crate::data::SubscribeFeed;
use crate::db::{self, Database};
use crate::network_frame::KookEventMessage;
use crate::network_runtime::BotNetworkEvent;
use crate::push::{push_info, push_post};
use crate::utils::{find_http_url, Throttle};
use crate::{fetch, push};
use futures_util::FutureExt;
use futures_util::StreamExt;
use log::*;
use once_cell::sync::Lazy;
use regex::Regex;
use std::cmp;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use thiserror::Error;
use tokio::sync::{broadcast, Notify};
use tokio_util::time::DelayQueue;

const COMMAND_RSS: &str = "/rss";
const COMMAND_SUB: &str = "/sub";
const COMMAND_UNSUB: &str = "/unsub";
const COMMAND_REG: &str = "/reg";

const SPACE: &str = " ";
const FEED_REFRESH_INTERVAL: u32 = 16;

#[derive(Error, Debug)]
pub enum KsbotError {
    #[error("不是一个有效的URL: {0}")]
    NotUrl(String),
    #[error("正则编译错误: {0}")]
    NotRegex(#[from] regex::Error),
    #[error("ksbot错误: {0}")]
    InnerRuntime(#[from] anyhow::Error),
    #[error("订阅错误: {0}")]
    Feed(#[from] fetch::FeedError),
    #[error("平台消息错误: {0}")]
    KookMessage(String),
    #[error("数据库错误: {0}")]
    Database(#[from] db::StoreError),
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
/unsub      - 退订一个 RSS: /unsub http://example.com/feed.xml
/reg        - 设置过滤正则: /reg http://example.com/feed.xml (华为|蒂法)
"
        .to_owned()
    }

    // 订阅
    async fn command_sub(&self, msg: &KookEventMessage, args: &[&str]) -> Result<(), KsbotError> {
        let url = args[0];

        let subscribe_url = match find_http_url(url) {
            Some(u) => u,
            None => return Err(KsbotError::NotUrl(url.to_owned())),
        };

        let channel = msg.target_id.to_owned().unwrap();
        let rss = fetch::pull_feed(subscribe_url).await?;
        info!("{} 订阅了 {}", channel, subscribe_url);
        let feed = SubscribeFeed::from(subscribe_url, &rss);
        self.db.channel_subscribed(&*channel, feed)?;
        push_info(&*format!("已订阅: {}", subscribe_url), msg).await?;
        if !&rss.posts.is_empty() {
            push_post(&channel, &rss.posts[0]).await?;
        }
        Ok(())
    }

    // 取消订阅
    async fn command_unsub(&self, msg: &KookEventMessage, args: &[&str]) -> Result<(), KsbotError> {
        let url = args[0];

        let subscribe_url = match find_http_url(url) {
            Some(u) => u,
            None => return Err(KsbotError::NotUrl(url.to_owned())),
        };

        let channel = msg.target_id.to_owned().unwrap();
        self.db.channel_unsubscribed(&*channel, subscribe_url)?;
        self.db.try_remove_feed(subscribe_url)?;
        push_info(&*format!("已取消订阅: {}", subscribe_url), msg).await?;
        Ok(())
    }

    async fn command_reg(&self, msg: &KookEventMessage, args: &[&str]) -> Result<(), KsbotError> {
        let url = args[0];
        let reg = args[1];

        let subscribe_url = match find_http_url(url) {
            Some(u) => u,
            None => return Err(KsbotError::NotUrl(url.to_owned())),
        };

        if let Err(e) = Regex::new(reg) {
            return Err(KsbotError::NotRegex(e));
        }
        let channel_id = msg.target_id.to_owned().unwrap();
        self.db
            .update_channel_feed_regex(&channel_id, subscribe_url, reg)?;

        push_info(&*format!("正则编译完成, 已启用."), msg).await?;
        Ok(())
    }

    async fn met_me(&self, msg: &KookEventMessage) -> Result<(), KsbotError> {
        push_info(&self.help(), msg).await?;
        Ok(())
    }

    async fn command_rss(&self, msg: &KookEventMessage) -> Result<(), KsbotError> {
        let channel_id = msg.target_id.to_owned().unwrap();
        let feeds = self.db.channel_feed_list(&*channel_id)?;
        let mut reply = "当前没有任何订阅, 是因为太年轻犯下的错么。".to_owned();

        if !feeds.is_empty() {
            let show_feeds = feeds
                .iter()
                .map(|s| format!("- [{}] {}", s.title, s.subscribe_url))
                .collect::<Vec<String>>();
            reply = show_feeds.join("\n");
        }

        push_info(&reply, msg).await?;
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
        if !is_valid_message(msg) {
            return Ok(());
        }

        info!(
            "channel_name = {:?}, nickname = {:?}, content = {:?}, type = {:?}, msg_timestrap = {:?} ",
            msg.channel_name, msg.nickname, msg.content, msg.typ, msg.msg_timestamp
        );

        let content = msg.content.to_owned().unwrap_or_else(|| "".to_owned());
        let channel_id = msg.target_id.to_owned().unwrap_or_else(|| "".to_owned());

        // 参数命令
        {
            if content.starts_with(COMMAND_SUB) {
                let args = content
                    .split(SPACE)
                    .filter(|t| !t.is_empty())
                    .collect::<Vec<&str>>();
                if args.len() == 2 && !channel_id.is_empty() {
                    self.command_sub(msg, &args[1..]).await?;
                    return Ok(());
                }
            }

            if content.starts_with(COMMAND_UNSUB) {
                let cmd_args = content
                    .split(SPACE)
                    .filter(|t| !t.is_empty())
                    .collect::<Vec<&str>>();
                if cmd_args.len() == 2 && !channel_id.is_empty() {
                    self.command_unsub(msg, &cmd_args[1..]).await?;
                    return Ok(());
                }
            }

            if content.starts_with(COMMAND_REG) {
                let cmd_args = content
                    .split(SPACE)
                    .filter(|t| !t.is_empty())
                    .collect::<Vec<&str>>();
                if cmd_args.len() == 3 && !channel_id.is_empty() {
                    self.command_reg(msg, &cmd_args[1..]).await?;
                    return Ok(());
                }
            }
        }

        // 无参数命令
        {
            match content.trim() {
                COMMAND_RSS => {
                    self.command_rss(msg).await?;
                    return Ok(());
                }
                _ => {}
            }
        }

        // 帮助说明，如果被@
        {
            // `@用户名` 这种信息在content中显示的格式是：`(met){用户ID}(met)` 这样的形式
            let met_me = format!("(met){}", self.me_info.as_ref().unwrap().id);
            if content.starts_with(&met_me) {
                self.met_me(msg).await?;
            }
        }

        Ok(())
    }
}

#[derive(Default)]
struct FetchQueue {
    feeds: HashMap<String, SubscribeFeed>,
    notifies: DelayQueue<String>,
    wakeup: Notify,
}

impl FetchQueue {
    fn new() -> Self {
        Self::default()
    }

    fn enqueue(&mut self, feed: SubscribeFeed, delay: Duration) -> bool {
        let exists = self.feeds.contains_key(&feed.subscribe_url);
        if !exists {
            self.notifies.insert(feed.subscribe_url.clone(), delay);
            self.feeds.insert(feed.subscribe_url.clone(), feed);
            self.wakeup.notify_waiters();
        }
        !exists
    }

    async fn next(&mut self) -> Result<SubscribeFeed, tokio::time::error::Error> {
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

fn is_valid_message(msg: &KookEventMessage) -> bool {
    match msg.msg_timestamp {
        Some(timestamp) => {
            let now = SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .expect("Time went backwards")
                .as_millis();

            if now > (timestamp + 5 * 1000) as u128 {
                info!("远古消息，忽略了: {:?}", msg);
                return false;
            }
        }
        None => {
            todo!()
        }
    }

    if let Some(bot) = msg.bot {
        if bot {
            info!("Bot消息，忽略...");
            return false;
        }
    }

    true
}
