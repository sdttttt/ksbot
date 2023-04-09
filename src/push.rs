use std::sync::Arc;

use crate::{api::http, network_frame::KookEventMessage, runtime::KsbotError};
use anyhow::bail;
use log::info;

use crate::{
    db::Database,
    fetch::{self, pull_feed},
    runtime::Feed,
};

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

    for ch in chans {
        let feed = old_feed.as_ref().unwrap();
        // 取出新的文章index
        let (new_indexs, _) = new_feed.diff_post_index(feed);
        if new_indexs.is_empty() {
            info!("文章无变化，不推送: {}", &*new_feed.subscribe_url);
        }
        for idx in new_indexs {
            push_post(ch.to_owned(), &new_rss.posts[idx]).await?;
        }
    }

    Ok(())
}

pub async fn push_post(
    chan_id: String,
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

    http::message_create(content, chan_id, None, None).await?;

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
