use anyhow::bail;
use std::sync::Arc;

use super::{is_http_ok, kook_api, KookResponse};

const GATEWAY_URL: &str = "/gateway/index";
const GATEWAY_DATA_KEY: &str = "url";

pub async fn get_wss_gateway(c: Arc<reqwest::Client>) -> Result<String, anyhow::Error> {
    let res = c.get(kook_api(GATEWAY_URL)).send().await?;

    let kres = res.json::<KookResponse>().await?;
    is_http_ok(&kres);

    let gateway_url = kres.data.get(GATEWAY_DATA_KEY);
    match gateway_url {
        Some(url) => Ok(url.clone()),
        None => bail!("kook响应成功，但是没有获得到Gateway?"),
    }
}
