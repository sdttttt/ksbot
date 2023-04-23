use std::{collections::HashMap, sync::Arc, time::Duration};

use anyhow::bail;
use once_cell::sync::OnceCell;
use reqwest::{header, Client};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use tokio::time::Interval;

use crate::conf::BotConfig;
use tracing::*;

use super::{is_http_ok, prefix_url, KookResponse};

const GATEWAY_URL: &str = "/gateway/index";
const GATEWAY_DATA_KEY: &str = "url";

const MESSAGE_CREATE_URL: &str = "/message/create";
const MESSAGE_TYPE_KMAEKDOWN: usize = 9;

const USER_ME_URL: &str = "/user/me";

static CLIENT: OnceCell<reqwest::Client> = OnceCell::new();
static CLIENT_SPEED_LIMIT: OnceCell<Arc<Mutex<Interval>>> = OnceCell::new();

pub fn init_kook_client(conf: BotConfig) {
    let http_client = {
        let mut headers = header::HeaderMap::new();
        headers.append(
            "Authorization",
            format!("Bot {}", conf.token).parse().unwrap(),
        );
        info!("header: {:?}", headers);
        Client::builder().default_headers(headers).build().unwrap()
    };

    // 请求速度限制器
    let interval = tokio::time::interval(Duration::from_millis(200));

    CLIENT
        .set(http_client)
        .expect("kook client already initialized");

    CLIENT_SPEED_LIMIT
        .set(Arc::new(Mutex::new(interval)))
        .expect("kook client speed limit  already initialized");
}

// 减速阀，每个请求平台API前调用这个
async fn req_slow_down() {
    let mut limit = CLIENT_SPEED_LIMIT.get().unwrap().lock().await;
    limit.tick().await;
}

pub async fn get_wss_gateway() -> Result<String, anyhow::Error> {
    req_slow_down().await;

    let url = &prefix_url(GATEWAY_URL);
    info!("get_wss_gateway: {}", url);
    let res = CLIENT
        .get()
        .expect("CLIENT not initialized")
        .get(url)
        .send()
        .await?;

    let kres = res.json::<KookResponse<HashMap<String, String>>>().await?;
    is_http_ok(&kres)?;

    let gateway_url = kres.data.get(GATEWAY_DATA_KEY);
    match gateway_url {
        Some(url) => Ok(url.clone()),
        None => bail!("kook响应成功，但是没有获得到Gateway?"),
    }
}

pub async fn message_create(
    content: String,
    target_id: String,
    typ: Option<usize>,
    quote: Option<String>,
) -> Result<(), anyhow::Error> {
    req_slow_down().await;

    #[derive(Debug, Serialize)]
    struct Request {
        content: String,
        target_id: String,
        #[serde(rename = "type")]
        typ: Option<usize>,
        quote: Option<String>,
    }

    let req = Request {
        content,
        target_id,
        typ,
        quote,
    };

    let res = CLIENT
        .get()
        .expect("CLIENT not initialized")
        .post(prefix_url(MESSAGE_CREATE_URL))
        .json(&req)
        .send()
        .await?;

    let kres = res.json::<KookResponse>().await?;
    is_http_ok(&kres)?;

    Ok(())
}

pub async fn user_me() -> Result<UserMe, anyhow::Error> {
    req_slow_down().await;

    let res = CLIENT
        .get()
        .expect("CLIENT not initialized")
        .get(prefix_url(USER_ME_URL))
        .send()
        .await?;
    let kres = res.json::<KookResponse<UserMe>>().await?;
    is_http_ok(&kres)?;
    Ok(kres.data)
}
#[derive(Debug, Serialize, Deserialize)]
pub struct UserMe {
    #[serde(rename = "id")]
    pub id: String,
    #[serde(rename = "username")]
    pub username: String,
    #[serde(rename = "identify_num")]
    pub identify_num: String,
    #[serde(rename = "online")]
    pub online: bool,
    #[serde(rename = "status")]
    pub status: i64,
    #[serde(rename = "avatar")]
    pub avatar: String,
    #[serde(rename = "bot")]
    pub bot: bool,
    #[serde(rename = "mobile_verified")]
    pub mobile_verified: bool,
    #[serde(rename = "client_id")]
    pub client_id: String,
    #[serde(rename = "mobile_prefix")]
    pub mobile_prefix: String,
    #[serde(rename = "mobile")]
    pub mobile: String,
    #[serde(rename = "invited_count")]
    pub invited_count: i64,
}
