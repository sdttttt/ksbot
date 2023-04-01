use log::*;
use sled::transaction::TransactionError;
use sled::IVec;
use thiserror::Error;

use crate::runtime::Feed;
use crate::utils;
const DEFAULT_DATABASE_PATH: &str = "__bot.db";
const ITEM_PAT: char = ';';

#[derive(Debug, Error)]
pub enum DatabaseError {
    #[error("数据库内部错误: {0}")]
    Inner(#[from] sled::Error),

    #[error("数据库内部事务错误：{0}")]
    InnerTransaction(#[from] TransactionError<sled::Error>),

    #[error("数据序列化错误: ")]
    Parse(#[from] serde_json::Error),
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
    pub fn channel_subscribed(&self, channel: &str, feed: Feed) -> Result<(), DatabaseError> {
        // 没有该订阅源先加入订阅列表
        if !self.contains_feed(&feed.link)? {
            self.update_or_create_feed(&feed)?;
        }

        let curr_feed_hash = feed_hash(&feed);
        // 频道的订阅列表全是以feed的link的hash表示
        self.chan_feed_operaiton(channel, |feeds| {
            if !feeds.contains(&curr_feed_hash) {
                feeds.push(curr_feed_hash.to_owned());
            }
        })?;

        self.feed_chan_operaiton(&*feed_channel_key(&feed.link), |chans| {
            if !chans.contains(&channel.to_owned()) {
                chans.push(channel.to_owned());
            }
        })?;

        Ok(())
    }

    // 频道取消订阅
    pub fn channel_unsubscribed(&self, channel: &str, link: &str) -> Result<(), DatabaseError> {
        let curr_feed_hash = utils::hash(link);
        self.feed_chan_operaiton(&*curr_feed_hash, |v| {
            for (idx, chan) in v.iter().enumerate() {
                if chan == channel {
                    v.remove(idx);
                    break;
                }
            }
        })?;

        self.chan_feed_operaiton(channel, |v| {
            for (idx, f) in v.iter().enumerate() {
                if f == &curr_feed_hash {
                    v.remove(idx);
                    break;
                }
            }
        })?;

        Ok(())
    }

    pub fn update_or_create_feed(&self, feed: &Feed) -> Result<Option<Feed>, DatabaseError> {
        let feed_v = serde_json::to_vec(feed)?;
        let feed_old = self.inner.insert(&*feed_key(&feed.link), feed_v)?;
        let result = feed_old.map(|t| {
            let s = utils::ivec_to_str(t);
            serde_json::from_str::<Feed>(&*s).expect("feed序列化失败")
        });

        Ok(result)
    }

    /// 尝试移除订阅源, 如果没有任何频道和该订阅源建立映射的话就会被移除
    /// 能移除订阅源的话可以减少部署端的订阅性能压力
    pub fn try_remove_feed(&self, feed_link: &str) -> Result<bool, DatabaseError> {
        let chan_list = self.feed_channel_list(feed_link)?;
        if chan_list.is_empty() {
            self.remove_feed(feed_link)?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub fn feed_list(&self) -> Result<Vec<Feed>, DatabaseError> {
        let iter = self.inner.scan_prefix(FEED_KEY_PREFIX);
        let mut feeds_text = Vec::<String>::new();
        for r in iter {
            match r {
                Ok((_, v_ivec)) => {
                    feeds_text.push(utils::ivec_to_str(v_ivec));
                }
                Err(e) => error!("遍历订阅源错误: {}", e),
            }
        }

        let feeds = feeds_text
            .iter()
            .map(|t| serde_json::from_str::<Feed>(t).expect("feed 反序列化错误"))
            .collect::<Vec<Feed>>();

        Ok(feeds)
    }

    /// 该频道的订阅列表
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
    pub fn feed_channel_list(&self, feed_link: &str) -> Result<Vec<String>, DatabaseError> {
        let feed_chan_key = &*feed_channel_key(feed_link);
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
        feed_chan_key: &str,
        f: impl FnMut(&mut Vec<String>),
    ) -> Result<(), DatabaseError> {
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
    fn contains_feed(&self, feed_link: &str) -> Result<bool, DatabaseError> {
        let r = self.inner.contains_key(feed_key(feed_link))?;
        debug!("contains_feed: {} > {}", r, feed_link);
        Ok(r)
    }

    // 移除订阅源
    #[inline]
    fn remove_feed(&self, feed_link: &str) -> Result<(), DatabaseError> {
        self.inner.remove(&*feed_key(feed_link))?;
        Ok(())
    }
}

const FEED_KEY_PREFIX: &str = "feed::";
#[inline]
fn feed_key(link: &str) -> String {
    let ha = utils::hash(link);
    format!("{}{}", FEED_KEY_PREFIX, ha)
}

const FEED_CHANNEL_KEY_PREFIX: &str = "feed::channel::";
#[inline]
fn feed_channel_key(link: &str) -> String {
    let ha = utils::hash(link);
    format!("{}{}", FEED_CHANNEL_KEY_PREFIX, ha)
}

const CHANNEL_FEED_KEY_PREFIX: &str = "channel::feed::";
#[inline]
fn channel_feed_key(channel: &str) -> String {
    format!("{}{}", CHANNEL_FEED_KEY_PREFIX, channel)
}

#[inline]
fn feed_hash(feed: &Feed) -> String {
    utils::hash(&feed.link)
}
