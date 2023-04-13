use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use crate::{
    api::http, fetch::item::ChannelItem, network_frame::KookEventMessage, runtime::KsbotError,
    utils,
};
use anyhow::bail;
use log::info;
use once_cell::sync::Lazy;
use regex::Regex;

use crate::{
    data::Feed,
    db::Database,
    fetch::{self, pull_feed},
};

// 存放已经编译好的正则表达式
static REGEX_FILTER_MAP: Lazy<Mutex<HashMap<String, Regex>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

pub async fn push_update(db: Arc<Database>, feed: Feed) -> Result<(), anyhow::Error> {
    info!("pull {}", &*feed.subscribe_url);
    let new_rss = match pull_feed(&*feed.subscribe_url).await {
        Ok(f) => f,
        Err(e) => bail!("Failed to pull feed: {:?}", e),
    };

    let new_feed = Feed::from(&feed.subscribe_url, &new_rss);
    let old_feed = db.update_or_create_feed(&new_feed)?;

    let chans = db.feed_channel_list(&*new_feed.subscribe_url)?;
    info!(
        "有{}个频道要推送: {}",
        chans.len(),
        &*new_feed.subscribe_url
    );

    let feed = old_feed.as_ref().unwrap();
    // 取出新的文章index
    let (ref new_indexs, _) = new_feed.diff_post_index(feed);
    if new_indexs.is_empty() {
        info!("文章无变化，不推送: {}", &*new_feed.subscribe_url);
    }

    for ch in chans {
        let regex_str_op = ch.feed_regex.get(&utils::hash(&feed.subscribe_url));
        for idx in new_indexs {
            let post = &new_rss.posts[*idx];

            // 是否需要过滤
            if let Some(reg_str) = regex_str_op {
                if !reg_str.trim().is_empty() && is_filter_post(post, reg_str) {
                    continue;
                }
            }

            push_post(&ch.id, post).await?;
        }
    }

    Ok(())
}

pub async fn push_post(
    chan_id: &str,
    item: &fetch::item::ChannelItem,
) -> Result<(), anyhow::Error> {
    if item.link.is_none() {
        return Ok(());
    }

    let content = format!(
        "{} - {}",
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

fn is_filter_post(t: &ChannelItem, reg_str: &str) -> bool {
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
