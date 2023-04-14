use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};

use crate::{
    api::http, fetch::item::FeedPost, network_frame::KookEventMessage, runtime::KsbotError, utils,
};
use anyhow::bail;
use log::*;
use once_cell::sync::Lazy;
use regex::Regex;

use crate::{
    data::SubscribeFeed,
    db::Database,
    fetch::{self, pull_feed},
};

// 存放已经编译好的正则表达式
static REGEX_FILTER_MAP: Lazy<Mutex<HashMap<String, Regex>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

pub async fn push_update(db: Arc<Database>, feed: SubscribeFeed) -> Result<(), anyhow::Error> {
    info!("pull {}", &*feed.subscribe_url);
    let new_rss = match pull_feed(&*feed.subscribe_url).await {
        Ok(f) => f,
        Err(e) => {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("Time went backwards")
                .as_secs();
            let expired = feed.down_time + 60 * 60 * 24 * 7; // 一周都不可用就是过期了.
            if expired < now {
                // TODO: 提醒该删除订阅源了
                warn!(
                    "该订阅源已经超过一周不可用了，建议删除: {}",
                    feed.subscribe_url
                );
            }
            bail!("Failed to pull feed: {:?}", e)
        }
    };

    let new_feed = SubscribeFeed::from_old(&feed, &new_rss);
    let old_feed = db.update_or_create_feed(&new_feed)?.unwrap(); // 更新

    // 取出新的文章index
    let ref new_indexs = new_feed.diff_post_index(&old_feed);
    if new_indexs.is_empty() {
        info!("订阅源无更新: {}", &*new_feed.subscribe_url);
        return Ok(());
    }

    info!("new: {:?},old: {:?}", new_feed, old_feed);

    let chans = db.feed_channel_list(&*new_feed.subscribe_url)?;
    for ch in chans {
        let regex_str_op = ch.feed_regex.get(&utils::hash(&old_feed.subscribe_url));
        for idx in new_indexs {
            let post = &new_rss.posts[*idx];

            // 是否需要过滤
            if let Some(reg_str) = regex_str_op {
                if !reg_str.trim().is_empty() && is_filter_post(post, reg_str) {
                    info!("被过滤的文章: {} match {:?}", reg_str, post.title);
                    continue;
                }
            }

            info!("推送: {:?} => {}", post.title, &ch.id);
            push_post(&ch.id, post).await?;
        }
    }

    Ok(())
}

pub async fn push_post(chan_id: &str, item: &fetch::item::FeedPost) -> Result<(), anyhow::Error> {
    if item.link.is_none() {
        return Ok(());
    }

    let content = format!(
        "**{}** \n > {}",
        item.title.as_ref().unwrap_or(&"".to_owned()),
        item.link.as_ref().unwrap()
    );

    http::message_create(content, chan_id.to_owned(), None, None).await?;

    Ok(())
}

pub async fn push_info(content: &str, msg: &KookEventMessage) -> Result<(), anyhow::Error> {
    let chan_id = msg.target_id.to_owned().unwrap();
    let quote = msg.msg_id.to_owned().unwrap();
    http::message_create(format!("{}", content), chan_id, None, Some(quote)).await?;

    Ok(())
}

pub async fn push_error(
    err: KsbotError,
    chan_id: String,
    quote: Option<String>,
) -> Result<(), anyhow::Error> {
    http::message_create(format!("{:?}", err), chan_id, None, quote).await?;

    Ok(())
}

fn is_filter_post(t: &FeedPost, reg_str: &str) -> bool {
    let mut filter_map = REGEX_FILTER_MAP.lock().unwrap();
    let reg = match filter_map.get(reg_str) {
        Some(reg) => reg,
        None => {
            let reg = Regex::new(reg_str).expect("新闻过滤正则编译错误，不可能");
            filter_map.insert(reg_str.to_owned(), reg);
            filter_map.get(reg_str).unwrap()
        }
    };

    let title = t.title.to_owned().unwrap_or_default();
    reg.is_match(&title)
}
