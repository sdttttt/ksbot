use std::time::Duration;

use once_cell::sync::OnceCell;
use thiserror::Error;

use super::{feed::RSSChannel, FromXmlWithBufRead};

static RESP_SIZE_LIMIT: OnceCell<u64> = OnceCell::new();
static CLIENT: OnceCell<reqwest::Client> = OnceCell::new();

const DEFAULT_RESP_SIZE_LIMIT: u64 = 1024 * 1024 * 4; // 4MB

#[derive(Error, Debug)]
pub enum FeedError {
    #[error("network error")]
    Network(#[from] reqwest::Error),
    #[error("feed parsing failed")]
    Parsing(#[from] quick_xml::Error),
    #[error("feed is too large")]
    TooLarge(u64),
}

pub async fn pull_feed(url: &str) -> Result<RSSChannel, FeedError> {
    let mut resp = CLIENT
        .get()
        .expect("CLIENT not initialized")
        .get(url)
        .send()
        .await?
        .error_for_status()?;

    let size_limit = *RESP_SIZE_LIMIT
        .get()
        .expect("RESP_SIZE_LIMIT not initialized");

    if let Some(len) = resp.content_length() {
        if len > size_limit {
            return Err(FeedError::TooLarge(size_limit));
        }
    }

    let rss = {
        let mut buf = vec![];
        while let Some(bytes) = resp.chunk().await? {
            if buf.len() + bytes.len() > size_limit as usize {
                return Err(FeedError::TooLarge(size_limit));
            }
            buf.extend_from_slice(&bytes);
        }
        super::RSSChannel::from_xml_with_buf(std::io::Cursor::new(buf))?
    };

    Ok(rss)
}

pub fn init_rss_client(max_feed_size: Option<u64>) {
    let client_builder = reqwest::Client::builder()
        .timeout(Duration::from_secs(16))
        .redirect(reqwest::redirect::Policy::limited(5))
        .user_agent("Mozilla/5.0")
        .danger_accept_invalid_certs(true);

    let client = client_builder.build().unwrap();

    CLIENT.set(client).expect("CLIENT already initialized");
    RESP_SIZE_LIMIT
        .set(max_feed_size.unwrap_or_else(|| DEFAULT_RESP_SIZE_LIMIT))
        .expect("RESP_SIZE_LIMIT already initialized");
}
#[cfg(test)]
mod test {

    use super::*;

    fn setup() {
        init_rss_client(None);
    }

    #[ignore]
    #[tokio::test]
    async fn test_pull_feed_for_yystv() {
        setup();
        let rss_chan = pull_feed("https://www.yystv.cn/rss/feed").await.unwrap();
        assert_eq!("游研社", rss_chan.title);
    }

    #[ignore]
    #[tokio::test]
    async fn test_pull_feed_for_rsshub_3dm() {
        setup();
        let rss_chan = pull_feed("https://rsshub.app/3dm/news").await.unwrap();
        assert_eq!("游研社", rss_chan.title);
    }
}
