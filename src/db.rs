use log::*;
use sled::transaction::TransactionError;
use sled::IVec;
use thiserror::Error;

use crate::data::{ChannelFeeds, Feed};
use crate::utils;
const DEFAULT_DATABASE_PATH: &str = "__bot.db";
const ITEM_PAT: char = ';';

#[derive(Debug, Error)]
pub enum DatabaseError {
    #[error("数据库内部错误: {0}")]
    Inner(#[from] sled::Error),

    #[error("数据库内部事务错误：{0}")]
    InnerTransaction(#[from] TransactionError<sled::Error>),

    #[error("数据序列化错误: {0}")]
    Parse(#[from] serde_json::Error),

    #[error("找不到订阅源: {0}")]
    NotFoundFeed(String),

    #[error("找不到频道信息: {0}")]
    NotFoundChannel(String),
}

#[derive(Debug)]
pub struct Database {
    inner: sled::Db,
}

impl Database {
    pub fn from_path(path: Option<String>) -> Self {
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
        if !self.contains_feed(&feed.subscribe_url)? {
            self.update_or_create_feed(&feed)?;
        }
        // 同上
        if !self.contains_channel(channel)? {
            self.update_or_create_channel(channel)?;
        }

        let curr_feed_hash = feed_hash(&feed);
        // 频道的订阅列表全是以feed的subscribe_url的hash表示
        self.chan_operaiton(&channel_key(channel), |chan_feeds| {
            if !chan_feeds.feed_hash.contains(&curr_feed_hash) {
                chan_feeds.feed_hash.push(curr_feed_hash.to_owned());
            }
        })?;

        self.feed_operaiton(&feed_key(&feed.subscribe_url), |feed| {
            if !feed.channel_ids.contains(&channel.to_owned()) {
                feed.channel_ids.push(channel.to_owned());
            }
        })?;

        Ok(())
    }

    // 频道取消订阅
    pub fn channel_unsubscribed(
        &self,
        channel: &str,
        subscribe_url: &str,
    ) -> Result<(), DatabaseError> {
        self.feed_operaiton(&feed_key(subscribe_url), |feed| {
            for (idx, chan) in feed.channel_ids.iter().enumerate() {
                if chan == channel {
                    feed.channel_ids.remove(idx);
                    break;
                }
            }
        })?;

        let curr_feed_hash = utils::hash(subscribe_url);
        self.chan_operaiton(&channel_key(channel), |chan| {
            for (idx, f) in chan.feed_hash.iter().enumerate() {
                if curr_feed_hash == *f {
                    chan.feed_hash.remove(idx);
                    break;
                }
            }
        })?;

        Ok(())
    }

    // 创建或者更新feed通过hash
    pub fn update_or_create_feed(&self, feed: &Feed) -> Result<Option<Feed>, DatabaseError> {
        let feed_v = serde_json::to_string(feed)?;
        let feed_old = self
            .inner
            .insert(&*feed_key(&feed.subscribe_url), &*feed_v)?;
        let result = feed_old.map(|t| {
            let s = utils::ivec_to_str(t);
            serde_json::from_str::<Feed>(&*s).expect("feed序列化失败")
        });

        Ok(result)
    }

    // 创建或者更新channelFeeds通过channel_id
    pub fn update_or_create_channel(
        &self,
        channel: &str,
    ) -> Result<Option<ChannelFeeds>, DatabaseError> {
        let chan_feeds_v = serde_json::to_string(&ChannelFeeds::from_id(channel.to_owned()))?;
        let old_chan_feeds_v = self.inner.insert(&*channel_key(channel), &*chan_feeds_v)?;

        let result = old_chan_feeds_v.map(|t| {
            let s = utils::ivec_to_str(t);
            serde_json::from_str::<ChannelFeeds>(&*s).expect("feed序列化失败")
        });

        Ok(result)
    }

    /// 尝试移除订阅源, 如果没有任何频道和该订阅源建立映射的话就会被移除
    /// 能移除订阅源的话可以减少部署端的订阅性能压力
    pub fn try_remove_feed(&self, subscribe_url: &str) -> Result<bool, DatabaseError> {
        let chan_list = self.feed_channel_list(subscribe_url)?;
        if chan_list.is_empty() {
            self.remove_feed(subscribe_url)?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub fn feed_list(&self) -> Result<Vec<Feed>, DatabaseError> {
        let iter = self.inner.scan_prefix(FEED_KEY_PREFIX);

        let feeds = iter
            // 过滤Err
            .filter_map(|t| t.ok())
            // 把value转成String
            .map(|t| utils::ivec_to_str(t.1))
            // 序列化
            .map(|t| serde_json::from_str::<Feed>(&t).expect("feed 反序列化错误"))
            .collect::<Vec<Feed>>();
        Ok(feeds)
    }

    /// 该频道的订阅列表
    pub fn channel_feed_list(&self, channel: &str) -> Result<Vec<Feed>, DatabaseError> {
        // 该订阅源的频道列表
        let mut feed_hashs: Vec<String> = vec![];
        match self.chan_operaiton(&channel_key(channel), |t| feed_hashs = t.feed_hash.clone()) {
            Ok(_) => {}
            Err(DatabaseError::NotFoundChannel(key)) => {
                error!("{}", DatabaseError::NotFoundChannel(key));
            }
            Err(e) => return Err(e),
        }

        let feed_keys = feed_hashs
            .iter()
            .map(|t| format!("{}{}", FEED_KEY_PREFIX, t))
            .collect::<Vec<String>>();

        let mut feeds = vec![];
        for k in feed_keys {
            if let Some(feed) = self.query_feed_by_key(&k)? {
                feeds.push(feed);
            }
        }

        Ok(feeds)
    }

    /// 该订阅源的频道列表
    pub fn feed_channel_list(
        &self,
        subscribe_url: &str,
    ) -> Result<Vec<ChannelFeeds>, DatabaseError> {
        let mut channel_ids: Vec<String> = vec![];
        match self.feed_operaiton(&feed_key(subscribe_url), |t| {
            channel_ids = t.channel_ids.clone()
        }) {
            Ok(_) => {}
            Err(DatabaseError::NotFoundFeed(key)) => {
                error!("{}", DatabaseError::NotFoundFeed(key));
            }
            Err(e) => return Err(e),
        }

        let chan_keys = channel_ids
            .iter()
            .map(|t| channel_key(t))
            .collect::<Vec<String>>();

        let mut chans = vec![];
        for k in chan_keys {
            if let Some(feed) = self.query_channel_by_id(&k)? {
                chans.push(feed);
            }
        }

        Ok(chans)
    }

    // 对频道的订阅列表操作，会写入
    fn chan_operaiton(
        &self,
        chan_key: &str,
        f: impl FnOnce(&mut ChannelFeeds),
    ) -> Result<(), DatabaseError> {
        // 该频道的订阅列表
        let chans_feed_ivec = self
            .inner
            .get(chan_key)?
            .unwrap_or_else(|| IVec::from(vec![]));
        // 转成 str
        let chans_feed_str = if chans_feed_ivec.is_empty() {
            return Err(DatabaseError::NotFoundChannel(chan_key.to_owned()));
        } else {
            utils::ivec_to_str(chans_feed_ivec)
        };

        let mut chan_feeds = serde_json::from_str::<ChannelFeeds>(&chans_feed_str)?;
        f(&mut chan_feeds);
        let post_chan_feeds_str = serde_json::to_string(&chan_feeds)?;

        self.inner.insert(chan_key, &*post_chan_feeds_str)?;
        Ok(())
    }

    // 对订阅源的频道列表操作，会写入
    fn feed_operaiton(
        &self,
        feed_key: &str,
        f: impl FnOnce(&mut Feed),
    ) -> Result<(), DatabaseError> {
        // 该订阅源的频道列表
        let feed_chans_ivec = self
            .inner
            .get(feed_key)?
            .unwrap_or_else(|| IVec::from(vec![]));
        // 转成 str
        let feed_chans_str = if feed_chans_ivec.is_empty() {
            return Err(DatabaseError::NotFoundFeed(feed_key.to_owned()));
        } else {
            utils::ivec_to_str(feed_chans_ivec)
        };

        let mut feed = serde_json::from_str::<Feed>(&feed_chans_str)?;
        f(&mut feed);
        let post_feed_str = serde_json::to_string(&feed)?;

        self.inner.insert(feed_key, &*post_feed_str)?;
        Ok(())
    }

    // 是否包含订阅源
    #[inline]
    pub fn contains_feed(&self, subscribe_url: &str) -> Result<bool, DatabaseError> {
        let r = self.inner.contains_key(feed_key(subscribe_url))?;
        info!("contains_feed: {} <=> {}", r, subscribe_url);
        Ok(r)
    }

    #[inline]
    pub fn contains_channel(&self, channel_id: &str) -> Result<bool, DatabaseError> {
        let r = self.inner.contains_key(channel_key(channel_id))?;
        info!("contains_channel: {} <=> {}", r, channel_id);
        Ok(r)
    }

    // 查询feed通过feed_key
    #[inline]
    fn query_feed_by_key(&self, feed_key: &str) -> Result<Option<Feed>, DatabaseError> {
        let feed_ivec_op = self.inner.get(feed_key)?;
        if let Some(feed_ivec) = feed_ivec_op {
            let feed_str = utils::ivec_to_str(feed_ivec);
            return Ok(Some(serde_json::from_str(&feed_str)?));
        }
        Ok(None)
    }

    // 查询channel通过chan_key
    #[inline]
    fn query_channel_by_id(&self, chan_key: &str) -> Result<Option<ChannelFeeds>, DatabaseError> {
        let chan_ivec_op = self.inner.get(chan_key)?;
        if let Some(chan_ivec) = chan_ivec_op {
            let chan_str = utils::ivec_to_str(chan_ivec);
            return Ok(Some(serde_json::from_str(&chan_str)?));
        }
        Ok(None)
    }

    // 移除订阅源
    #[inline]
    fn remove_feed(&self, subscribe_url: &str) -> Result<(), DatabaseError> {
        self.inner.remove(&*feed_key(subscribe_url))?;
        Ok(())
    }
}

// feed::{subscribe_url_hash} = {Feed Struct}
const FEED_KEY_PREFIX: &str = "feed::";
#[inline]
fn feed_key(subscribe_url: &str) -> String {
    let ha = utils::hash(subscribe_url);
    format!("{}{}", FEED_KEY_PREFIX, ha)
}

// channel::{channel_id} = {ChannelFeeds Struct}
const CHANNEL_KEY_PREFIX: &str = "channel::";
#[inline]
fn channel_key(channel: &str) -> String {
    format!("{}{}", CHANNEL_KEY_PREFIX, channel)
}

#[inline]
fn feed_hash(feed: &Feed) -> String {
    utils::hash(&feed.subscribe_url)
}

#[cfg(test)]
mod test {

    use crate::data::Feed;

    use super::Database;
    use once_cell::sync::Lazy;

    const TEST_DB_PATH: &str = "__bot_test.db";

    static DB: Lazy<Database> = Lazy::new(|| Database::from_path(Some(TEST_DB_PATH.to_owned())));

    fn before_setup() {
        if std::path::Path::new(TEST_DB_PATH).exists() {
            std::fs::remove_dir_all(TEST_DB_PATH).unwrap();
        }
    }

    #[test]
    fn test_db() {
        before_setup();

        let link = "http://a.b";
        let subscribe_url = "http://b.a";
        let chan = "test_chan";
        let feed = Feed {
            title: "test_feed".to_owned(),
            link: link.to_owned(),
            subscribe_url: subscribe_url.to_owned(),
            ..Default::default()
        };

        let feeds = DB.channel_feed_list(chan).unwrap();
        assert_eq!(0, feeds.len());
        let chans = DB.feed_channel_list(subscribe_url).unwrap();
        assert_eq!(0, chans.len());

        DB.channel_subscribed(chan, feed.to_owned()).unwrap();

        let feeds_1 = DB.channel_feed_list(chan).unwrap();
        assert_eq!(1, feeds_1.len());
        assert_eq!("http://b.a", feeds_1[0].subscribe_url);
        let chans_1 = DB.feed_channel_list(subscribe_url).unwrap();
        assert_eq!(1, chans_1.len());
        assert_eq!(chan, chans_1[0].id);

        let all_feeds = DB.feed_list().unwrap();
        assert_eq!(1, all_feeds.len());

        DB.channel_unsubscribed("test_chan", &feed.subscribe_url)
            .unwrap();

        let feeds_2 = DB.channel_feed_list(chan).unwrap();
        assert_eq!(0, feeds_2.len());
        let chans_2 = DB.feed_channel_list(subscribe_url).unwrap();
        assert_eq!(0, chans_2.len());

        let feeds_list = DB.feed_list().unwrap();
        assert_eq!(feeds_list.len(), 1);

        assert!(DB.contains_feed(subscribe_url).unwrap());
        DB.try_remove_feed(subscribe_url).unwrap();
        assert!(!DB.contains_feed(subscribe_url).unwrap());

        let feeds_list = DB.feed_list().unwrap();
        assert_eq!(feeds_list.len(), 0);
    }

    #[test]
    fn test_serde() {
        let link = "http://a.b";
        let feed = Feed {
            title: "test_feed".to_owned(),
            link: link.to_owned(),
            ..Default::default()
        };

        let json = serde_json::to_string(&feed).unwrap();
        assert_ne!(json, "".to_owned());
        let feed2 = serde_json::from_str::<Feed>(&json).unwrap();
        assert_eq!(link, feed2.link);
        assert_eq!("test_feed", feed2.title);
    }
}
