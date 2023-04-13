use std::{
    collections::HashMap,
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};

use crate::{fetch::feed::RSSChannel, utils};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Feed {
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

impl Feed {
    pub fn from(url: &str, rss: &RSSChannel) -> Self {
        let posts_hash = rss
            .posts
            .iter()
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

    // 返回对比的两组feed, post_hash 不同的文章哈希
    // result.0 调用方 result.1 是参数方
    pub fn diff_post_index(&self, feed: &Feed) -> (Vec<usize>, Vec<usize>) {
        let ph_1 = &self.posts_hash;
        let ph_2 = &feed.posts_hash;
        let ph_1_diff = ph_1
            .iter()
            .enumerate()
            .filter(|&t| !ph_2.contains(t.1))
            .map(|t| t.0)
            .collect();

        let ph_2_diff = ph_2
            .iter()
            .enumerate()
            .filter(|&t| !ph_1.contains(t.1))
            .map(|t| t.0)
            .collect();

        (ph_1_diff, ph_2_diff)
    }
}

impl TryFrom<&Feed> for String {
    type Error = serde_json::Error;

    fn try_from(value: &Feed) -> Result<Self, Self::Error> {
        let r = serde_json::to_string(value)?;
        Ok(r)
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ChannelFeeds {
    pub id: String,
    pub feed_hash: Vec<String>,
    pub feed_regex: HashMap<String, String>,
}

impl ChannelFeeds {
    pub fn from_id(id: String) -> Self {
        Self {
            id,
            feed_hash: vec![],
            feed_regex: HashMap::new(),
        }
    }
}

impl TryFrom<&ChannelFeeds> for String {
    type Error = serde_json::Error;

    fn try_from(value: &ChannelFeeds) -> Result<Self, Self::Error> {
        let r = serde_json::to_string(value)?;
        Ok(r)
    }
}
