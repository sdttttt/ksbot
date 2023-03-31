use log::*;
use once_cell::sync::Lazy;
use sled::transaction::TransactionError;
use sled::IVec;
use std::collections::hash_map::DefaultHasher;
use std::hash::Hash;
use std::hash::Hasher;
use std::sync::Mutex;
use thiserror::Error;

use crate::fetch;
use crate::utils;
const DEFAULT_DATABASE_PATH: &str = "__bot.db";
const ITEM_PAT: char = ';';

static HASHER: Lazy<Mutex<DefaultHasher>> = Lazy::new(|| Mutex::new(DefaultHasher::new()));

#[derive(Debug, Error)]
pub enum DatabaseError {
    #[error("订阅源检查出错: {0}")]
    Feed(#[from] fetch::FeedError),

    #[error("数据库内部错误: {0}")]
    Inner(#[from] sled::Error),

    #[error("数据库内部事务错误：{0}")]
    InnerTransaction(#[from] TransactionError<sled::Error>),
}

#[derive(Debug)]
pub struct Database {
    inner: sled::Db,
}

impl Database {
    pub fn new(path: Option<String>) -> Self {
        let path = path.unwrap_or_else(|| DEFAULT_DATABASE_PATH.to_owned());

        let inner = sled::Config::default()
            .flush_every_ms(Some(4 * 1000))
            .path(path)
            .open()
            .expect(&format!(
                "数据库初始化出错, 反复出现此错误可以尝试删除数据库文件。(默认: {})",
                DEFAULT_DATABASE_PATH
            ));

        Database { inner }
    }

    // 频道订阅
    pub fn channel_subscribed(&self, channel: &str, feed: &str) -> Result<(), DatabaseError> {
        // 没有该订阅源先加入订阅列表
        if !self.contains_feed(feed)? {
            self.insert_feed(feed)?;
        }

        self.chan_feed_operaiton(channel, |feeds| {
            if !feeds.contains(&feed.to_owned()) {
                feeds.push(feed.to_owned());
            }
        })?;

        self.feed_chan_operaiton(feed, |chans| {
            if !chans.contains(&channel.to_owned()) {
                chans.push(channel.to_owned());
            }
        })?;

        Ok(())
    }

    // 频道取消订阅
    pub fn channel_unsubscribed(&self, channel: &str, feed: &str) -> Result<(), DatabaseError> {
        self.feed_chan_operaiton(feed, |v| {
            for (idx, chan) in v.iter().enumerate() {
                if chan == channel {
                    v.remove(idx);
                    break;
                }
            }
        })?;

        self.chan_feed_operaiton(channel, |v| {
            for (idx, f) in v.iter().enumerate() {
                if f == feed {
                    v.remove(idx);
                    break;
                }
            }
        })?;

        Ok(())
    }

    fn insert_feed(&self, feed: &str) -> Result<(), DatabaseError> {
        self.inner.insert(&*feed_key(feed), feed)?;
        Ok(())
    }

    /// 尝试移除订阅源, 如果没有任何频道和该订阅源建立映射的话就会被移除
    /// 能移除订阅源的话可以减少部署端的订阅性能压力
    pub fn try_remove_feed(&self, feed: &str) -> Result<bool, DatabaseError> {
        let chan_list = self.feed_channel_list(feed)?;
        if chan_list.is_empty() {
            self.remove_feed(feed)?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// 该订阅源的频道列表
    pub fn channel_feed_list(&self, channel: &str) -> Result<Vec<String>, DatabaseError> {
        let chan_feed_key = &*channel_feed_key(channel);
        // 该订阅源的频道列表
        let chan_feeds_ivec = self
            .inner
            .get(chan_feed_key)?
            .unwrap_or_else(|| IVec::from(vec![]));
        // 转成 str
        let chan_feeds_str = if chan_feeds_ivec.is_empty() {
            return Ok(vec![]);
        } else {
            utils::ivec_to_str(chan_feeds_ivec)
        };

        Ok(utils::split_vec_filter_empty(chan_feeds_str, ITEM_PAT))
    }

    /// 该订阅源的频道列表
    pub fn feed_channel_list(&self, feed: &str) -> Result<Vec<String>, DatabaseError> {
        let feed_chan_key = &*feed_channel_key(feed);
        // 该订阅源的频道列表
        let feed_chans_ivec = self
            .inner
            .get(feed_chan_key)?
            .unwrap_or_else(|| IVec::from(vec![]));
        // 转成 str
        let feed_chans_str = if feed_chans_ivec.is_empty() {
            return Ok(vec![]);
        } else {
            utils::ivec_to_str(feed_chans_ivec)
        };

        Ok(utils::split_vec_filter_empty(feed_chans_str, ITEM_PAT))
    }

    // 对频道的订阅列表操作，会写入
    fn chan_feed_operaiton(
        &self,
        channel: &str,
        f: impl FnMut(&mut Vec<String>),
    ) -> Result<(), DatabaseError> {
        let chan_feed_key = &*channel_feed_key(channel);
        // 该频道的订阅列表
        let chans_feed_ivec = self
            .inner
            .get(chan_feed_key)?
            .unwrap_or_else(|| IVec::from(vec![]));
        // 转成 str
        let chans_feed_str = if chans_feed_ivec.is_empty() {
            "".to_owned()
        } else {
            utils::ivec_to_str(chans_feed_ivec)
        };

        let chans_feed_str_processed =
            utils::split_filter_empty_join_process(chans_feed_str, ITEM_PAT, f);

        self.inner
            .insert(chan_feed_key, &*chans_feed_str_processed)?;
        Ok(())
    }

    // 对订阅源的频道列表操作，会写入
    fn feed_chan_operaiton(
        &self,
        feed: &str,
        f: impl FnMut(&mut Vec<String>),
    ) -> Result<(), DatabaseError> {
        let feed_chan_key = &*feed_channel_key(feed);
        // 该订阅源的频道列表
        let feed_chans_ivec = self
            .inner
            .get(feed_chan_key)?
            .unwrap_or_else(|| IVec::from(vec![]));
        // 转成 str
        let feed_chans_str = if feed_chans_ivec.is_empty() {
            "".to_owned()
        } else {
            utils::ivec_to_str(feed_chans_ivec)
        };
        let feed_chans_str_processed =
            utils::split_filter_empty_join_process(feed_chans_str, ITEM_PAT, f);
        self.inner
            .insert(feed_chan_key, &*feed_chans_str_processed)?;
        Ok(())
    }

    // 是否包含订阅源
    #[inline]
    fn contains_feed(&self, feed: &str) -> Result<bool, DatabaseError> {
        let r = self.inner.contains_key(feed_key(feed))?;
        debug!("contains_feed: {} > {}", r, feed);
        Ok(r)
    }

    // 移除订阅源
    #[inline]
    fn remove_feed(&self, feed: &str) -> Result<(), DatabaseError> {
        let feed_chan_key = &*feed_channel_key(feed);
        self.inner.remove(feed_chan_key)?;
        Ok(())
    }
}

fn feed_key(feed: &str) -> String {
    let ha = hash(feed);
    format!("feed::{}", ha)
}

fn feed_channel_key(feed: &str) -> String {
    let ha = hash(feed);
    format!("feed::channel:{}", ha)
}

fn channel_feed_key(channel: &str) -> String {
    format!("channel::feed:{}", channel)
}

fn feed_date_key(feed: &str) -> String {
    let ha = hash(feed);
    format!("feed::date::{}", ha)
}

fn hash(k: impl Hash) -> String {
    let mut hasher = HASHER.lock().unwrap();
    k.hash(&mut *hasher);
    hasher.finish().to_string()
}
