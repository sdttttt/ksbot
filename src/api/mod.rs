use anyhow::bail;
use serde::Deserialize;
use serde_json::Value;
pub mod http;

const KOOK_BASE_API: &str = "https://www.kookapp.cn/api/v3";
const KOOK_OK_CODE: usize = 0;

#[derive(Debug, Deserialize)]
struct KookResponse<T = Value> {
    code: usize,
    message: String,
    data: T,
}

fn prefix_url(url: &str) -> String {
    format!("{}{}", KOOK_BASE_API, url)
}

fn not_compress(url: &str) -> String {
    format!("{}?compress=0", url)
}

fn is_http_ok<T>(kres: &KookResponse<T>) -> Result<(), anyhow::Error> {
    if kres.code != KOOK_OK_CODE {
        bail!(kres.message.to_owned())
    }
    Ok(())
}
