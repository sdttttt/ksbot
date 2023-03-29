use anyhow::bail;
use reqwest::{header, Client};

use crate::conf::BotConfig;

use super::{is_http_ok, kook_api, KookResponse};


const GATEWAY_URL: &str = "/gateway/index";
    
const GATEWAY_DATA_KEY: &str = "url";

pub struct KookHttpClient {
    c: reqwest::Client,
}

impl KookHttpClient {
    pub fn new(conf: &BotConfig) -> KookHttpClient {
        let http_client = {
            let mut headers = header::HeaderMap::new();
            headers.append(
                "Authorization",
                format!("Bot {}", conf.token).parse().unwrap(),
            );
            println!("header: {:?}", headers);
            Client::builder().default_headers(headers).build().unwrap()
        };

        Self {
            c: http_client
        }
    }

    pub async fn get_wss_gateway(&self) -> Result<String, anyhow::Error> {
        let url = &kook_api(GATEWAY_URL);
        println!("get_wss_gateway: {}", url);
        let res = self.c.get(url).send().await?;
    
        let kres = res.json::<KookResponse>().await?;
        is_http_ok(&kres)?;
    
        let gateway_url = kres.data.get(GATEWAY_DATA_KEY);
        match gateway_url {
            Some(url) => Ok(url.clone()),
            None => bail!("kook响应成功，但是没有获得到Gateway?"),
        }
    }
}


