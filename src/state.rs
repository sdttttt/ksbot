use crate::api::http::get_wss_gateway;
use crate::conf::Config;
use futures_util::{
    stream::{SplitSink, SplitStream, StreamExt},
    SinkExt,
};

use reqwest::{header, Client};
use std::{sync::Arc, time::Duration};
use tokio::{net::TcpStream, time::sleep};
use tokio_tungstenite::{connect_async, tungstenite::Message, MaybeTlsStream, WebSocketStream};

enum BotState {
    GetGateway,     // 初始化：尝试获取ws网关
    ConnectGateway, // 已经获取网关：尝试连接上网关
    Working,        // 工作中：保持心跳
    Timeout,        // 超时：尝试重试
}

// 机器人运行时：这里没有任何实现功能的代码，只有机器人和服务器保持连接的状态机代码。
pub struct BotRuntime {
    state: BotState,
    conf: Arc<Config>,

    http_client: Arc<reqwest::Client>,

    gateway_url: Option<String>,
    ws_write: Option<SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, Message>>,
    ws_read: Option<SplitStream<WebSocketStream<MaybeTlsStream<TcpStream>>>>,
}

impl BotRuntime {
    pub fn init(conf: Config) -> Self {
        let conf = Arc::new(conf);
        let http_client = {
            let mut headers = header::HeaderMap::new();
            headers.append(
                "Authorization",
                format!("Bot {}", conf.token).parse().unwrap(),
            );
            Arc::new(Client::builder().build().unwrap())
        };

        Self {
            state: BotState::GetGateway,
            conf,
            http_client,
            gateway_url: None,
            ws_read: None,
            ws_write: None,
        }
    }

    pub async fn run(&mut self) -> Result<(), anyhow::Error> {
        let mut connect_gateway_count = 0;

        loop {
            match self.state {
                BotState::GetGateway => match self.try_get_gateway().await {
                    Ok(_) => {
                        self.state = BotState::ConnectGateway;
                        println!("获取网关成功，尝试连接...")
                    }
                    Err(e) => {
                        println!("获取网关失败: {}", e);
                        sleep(Duration::from_secs(4)).await;
                    }
                },

                BotState::ConnectGateway => match self.connect_wss_server().await {
                    Ok(_) => {
                        self.state = BotState::Working;
                        // 成功后，重置连接网关计数
                        connect_gateway_count = 0;
                        println!("连接网关成功，等待服务器响应.")
                    }
                    Err(e) => {
                        println!("连接网关失败: {}", e);
                        if connect_gateway_count > 2 {
                            println!("连接网关已到最大重试次数, 重新获取网关.");
                            self.state = BotState::GetGateway;
                        }
                        connect_gateway_count += 1;
                        sleep(Duration::from_secs(4)).await;
                    }
                },

                BotState::Working => todo!(),

                BotState::Timeout => todo!(),
            };
        }
    }

    pub async fn try_get_gateway(&mut self) -> Result<(), anyhow::Error> {
        let client = self.http_client.clone();
        let wss_url = get_wss_gateway(client).await?;
        self.gateway_url = Some(wss_url);
        Ok(())
    }

    pub async fn connect_wss_server(&mut self) -> Result<(), anyhow::Error> {
        let ws_url = url::Url::parse(self.gateway_url.as_ref().unwrap())?;
        let (wconn, _) = connect_async(ws_url).await?;
        println!("WebSocket handshake has been successfully completed");

        let (write, read) = wconn.split();
        self.ws_read = Some(read);
        self.ws_write = Some(write);
        todo!()
    }
}
