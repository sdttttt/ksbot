use anyhow::bail;
use lazy_static::lazy_static;
use reqwest::{header, Client};
use serde::Deserialize;
use std::{collections::HashMap, fmt::format, iter::Map};
pub mod http;

const KOOK_BASE_API: &str = "https://www.kookapp.cn/api/v3";
const KOOK_OK_CODE: usize = 0;

#[derive(Debug, Deserialize)]
struct KookResponse<T = HashMap<String, String>> {
    code: usize,
    message: String,
    data: T,
}

fn kook_api(url: &str) -> String {
    format!("{}{}", KOOK_BASE_API, url)
}

fn not_compress(url: &str) -> String {
    format!("{}?compress=0", url)
}

fn is_http_ok(kres: &KookResponse) -> Result<(), anyhow::Error> {
    if kres.code != KOOK_OK_CODE {
        bail!(kres.message.to_owned())
    }
    Ok(())
}
