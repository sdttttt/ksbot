use std::{sync::Arc, time::Duration};

use crate::api::http;
use anyhow::bail;

use crate::{
    db::Database,
    fetch::{self, pull_feed},
    runtime::Feed,
};

pub async fn pull_feed_and_push_update(db: Arc<Database>, feed: Feed) -> Result<(), anyhow::Error> {
    let new_rss = match pull_feed(&*feed.link).await {
        Ok(f) => f,
        Err(e) => bail!("Failed to pull feed: {:?}", e),
    };

    let new_feed = Feed::from(&new_rss);
    let old_feed = db.update_or_create_feed(&new_feed)?;

    if new_feed.posts_hash.is_empty() {
        return Ok(());
    }

    let chans = db.feed_channel_list(&*new_feed.link)?;

    for ch in chans {
        tokio::time::sleep(Duration::from_millis(200)).await;
        match old_feed {
            None => {
                // 第一次刷新订阅源，就推送一条最新的新闻。
                push_update(ch, &new_rss.posts[0]).await?;
            }
            Some(ref feed) => {
                // 取出新的文章index
                let (new_indexs, _) = new_feed.diff_post_index(feed);
                for idx in new_indexs {
                    push_update(ch.to_owned(), &new_rss.posts[idx]).await?;
                }
            }
        }
    }

    Ok(())
}

async fn push_update(
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
