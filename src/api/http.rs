use anyhow::bail;
use reqwest::{header, Client};
use serde::Serialize;

use crate::conf::BotConfig;

use super::{is_http_ok, prefix_url, KookResponse};

const GATEWAY_URL: &str = "/gateway/index";
const GATEWAY_DATA_KEY: &str = "url";

const MESSAGE_CREATE_URL: &str = "/message/create";
const MESSAGE_TYPE_KMAEKDOWN: usize = 9;

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
        

        let url = &prefix_url(GATEWAY_URL);
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


    pub async fn message_create(&self,  content: String, target_id: String, typ: Option<usize>, quote: Option<String>) -> Result<(), anyhow::Error> {

        #[derive(Debug, Serialize)]
        struct Request {
            content: String,
            target_id: String,
            #[serde(rename = "type")]
            typ: Option<usize>,
            quote: Option<String>
        }

        let req = Request {
             content, target_id, typ, quote
        };

        let res = self.c.post (prefix_url( MESSAGE_CREATE_URL)).json(&req).send().await?;

        let kres = res.json::<KookResponse>().await?;
        is_http_ok(&kres)?;

        Ok(())
    }
}


