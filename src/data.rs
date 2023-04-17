use std::{
    collections::HashMap,
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};

use crate::{fetch::feed::Feed, utils};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SubscribeFeed {
    // 订阅URL
    pub subscribe_url: String,
    // link
    pub link: String,
    // 标题
    pub title: String,
    // 最后一次订阅时间
    pub down_time: u64,
    pub ttl: Option<u32>,
    pub posts_hash: Vec<String>,
    pub channel_ids: Vec<String>,
}

const POSTS_HASH_MAX: usize = 16; // 最长存放15个

impl SubscribeFeed {
    pub fn from(url: &str, rss: &Feed) -> Self {
        let posts_hash = rss
            .posts
            .iter()
            .take(POSTS_HASH_MAX)
            .map(|t| utils::hash(&t.link.as_ref().unwrap()))
            .collect();

        let start = SystemTime::now();
        let since_the_epoch = start
            .duration_since(UNIX_EPOCH)
            .expect("Time went backwards");

        Self {
            subscribe_url: url.to_owned(),
            title: rss.title.to_owned(),
            link: rss.link.to_owned(),
            down_time: since_the_epoch.as_secs(),
            ttl: rss.ttl,
            posts_hash,
            channel_ids: vec![],
        }
    }

    pub fn from_old(old: &Self, rss: &Feed) -> Self {
        let posts_hash = rss
            .posts
            .iter()
            .take(POSTS_HASH_MAX)
            .map(|t| utils::hash(t.link.as_ref().unwrap()))
            .collect();

        let start = SystemTime::now();
        let since_the_epoch = start
            .duration_since(UNIX_EPOCH)
            .expect("Time went backwards");

        Self {
            subscribe_url: old.subscribe_url.to_owned(),
            title: rss.title.to_owned(),
            link: rss.link.to_owned(),
            down_time: since_the_epoch.as_secs(),
            ttl: rss.ttl,
            posts_hash,
            channel_ids: old.channel_ids.to_owned(),
        }
    }

    // 对比 post_hash 不同的文章哈希
    // 返回下标
    pub fn diff_post_index(&self, feed: &SubscribeFeed) -> Vec<usize> {
        let ph_1 = &self.posts_hash;
        let ph_2 = &feed.posts_hash;
        let diff = ph_1
            .iter()
            .enumerate()
            .filter(|t| !ph_2.contains(t.1))
            .map(|t| t.0)
            .collect();
        diff
    }
}

impl TryFrom<&SubscribeFeed> for String {
    type Error = serde_json::Error;

    fn try_from(value: &SubscribeFeed) -> Result<Self, Self::Error> {
        let r = serde_json::to_string(value)?;
        Ok(r)
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ChannelSubFeeds {
    pub id: String,
    pub feed_hash: Vec<String>,
    // K = feed_hash V = regex expression
    pub feed_regex: HashMap<String, String>,
}

impl ChannelSubFeeds {
    pub fn from_id(id: String) -> Self {
        Self {
            id,
            feed_hash: vec![],
            feed_regex: HashMap::new(),
        }
    }
}

impl TryFrom<&ChannelSubFeeds> for String {
    type Error = serde_json::Error;

    fn try_from(value: &ChannelSubFeeds) -> Result<Self, Self::Error> {
        let r = serde_json::to_string(value)?;
        Ok(r)
    }
}
