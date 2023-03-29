use crate::api::http::KookHttpClient;
use crate::conf::BotConfig;
use crate::ws::{
    KookWSFrame, WS_DATA_CODE_FIELD, WS_DATA_CODE_INVAILD_TOKEN, WS_DATA_CODE_MISS_PARAM,
    WS_DATA_CODE_OK, WS_DATA_CODE_TOKEN_EXPIRE, WS_DATA_CODE_TOKEN_VALID_FAIL,
    WS_DATA_HELLO_SESSION_ID_FIELD, WS_MESSAGE, WS_PONG, WS_RECONNECT, WS_RESUME_ACK,KookChannelMessage, WS_HELLO
};
use anyhow::bail;
use flate2::read::ZlibDecoder;
use futures_util::{
    stream::{SplitSink, SplitStream, StreamExt},
    SinkExt,
};

use serde_json::Value;
use std::collections::HashMap;
use std::io::Read;
use std::{sync::Arc, time::Duration};
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::time::timeout;
use tokio::{net::TcpStream, time::sleep};
use tokio_tungstenite::{connect_async, tungstenite::Message, MaybeTlsStream, WebSocketStream};

enum BotState {
    GetGateway,     // 初始化：尝试获取ws网关
    ConnectGateway, // 已经获取网关：尝试连接上网关
    WaitHello,      // 等待服务端Hello，包
    Working,        // 工作中：保持心跳
    HeartTimeout,   // 心跳超时：恢复到ConnectGateway
    Reconnect,      // 重新连接：恢复到GetGateway
}

// 机器人运行时，基础设施
pub struct BotRuntime {
    state: BotState,
    conf: BotConfig,
    // 网关
    gateway_url: Option<String>,
    // 会话ID
    session_id: Option<String>,
    sn: u64,

    http_client: Arc<KookHttpClient>,

    /// WebSocket  connection
    ws_write: Option<SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, Message>>,
    ws_read: Option<SplitStream<WebSocketStream<MaybeTlsStream<TcpStream>>>>,

    // 消息处理信道
    process_msg_chan: (Sender<KookWSFrame<KookChannelMessage>>, Receiver<KookWSFrame<KookChannelMessage>>),
    // 待处理的消息，存放乱序消息
    wait_processing_msg_map: HashMap<u64, KookWSFrame<KookChannelMessage>>,

    /// 心跳信道
    heart_channel: (Sender<bool>, Receiver<bool>),
}

impl BotRuntime {
    pub fn init(conf: BotConfig) -> Self {
        let http_client = Arc::new(KookHttpClient::new(&conf));

        Self {
            state: BotState::GetGateway,
            conf,
            http_client,
            gateway_url: None,
            session_id: None,
            sn: 0,
            ws_read: None,
            ws_write: None,
            process_msg_chan: tokio::sync::mpsc::channel::<KookWSFrame<KookChannelMessage>>(64),
            wait_processing_msg_map: HashMap::new(),
            heart_channel: tokio::sync::mpsc::channel::<bool>(1),
        }
    }

    pub async fn run(&mut self) -> Result<(), anyhow::Error> {
        // 网关连接计数器
        let mut connect_gateway_count = 0;

        loop {
            match self.state {
                BotState::GetGateway | BotState::Reconnect => match self.try_get_gateway().await {
                    Ok(_) => {
                        self.state = BotState::ConnectGateway;
                        println!("获取网关成功，尝试连接...")
                    }
                    Err(e) => {
                        println!("获取网关失败: {}", e);
                        sleep(Duration::from_secs(4)).await;
                    }
                },

                BotState::ConnectGateway | BotState::HeartTimeout => {
                    match self.connect_wss_server().await {
                        Ok(_) => {
                            self.state = BotState::WaitHello;
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
                    }
                }

                BotState::WaitHello => {
                    let wait_hello_f = self.wait_hello();
                    // 设定超时时间，必须在6秒之内完成wait_hello
                    match timeout(Duration::from_secs(6), wait_hello_f).await {
                        Ok(result) => match result {
                            Ok(_) => {
                                self.state = BotState::Working;
                                println!("接受完成Hello包，开始工作.");
                            }
                            Err(e) => {
                                println!("接收Hello信令失败: {}", e);
                                self.state = BotState::GetGateway;
                                sleep(Duration::from_secs(4)).await;
                            }
                        },
                        Err(e) => {
                            println!("等待Hello信令超时: {}", e);
                            self.state = BotState::GetGateway;
                        }
                    }
                }

                BotState::Working => {
                    match self.work().await {
                        Err(e) => println!("工作出错: {:?}", e),
                        _ => (),
                    }

                    self.state = BotState::GetGateway;
                }
            };
        }
    }

    // 获取网关
    async fn try_get_gateway(&mut self) -> Result<(), anyhow::Error> {
        println!("try_get_gateway");
        let wss_url = self.http_client.get_wss_gateway().await?;
        println!("wss_url: {}", wss_url);
        self.gateway_url = Some(wss_url);
        Ok(())
    }

    // 连接到WS网关
    async fn connect_wss_server(&mut self) -> Result<(), anyhow::Error> {
        println!("connect_wss_server");
        let ws_url = url::Url::parse(self.gateway_url.as_ref().unwrap())?;
        let (wconn, _) = connect_async(ws_url).await?;
        let (write, read) = wconn.split();

        self.ws_read = Some(read);
        self.ws_write = Some(write);
        Ok(())
    }

    // 等待服务器的Hello包，获取其中的session
    async fn wait_hello(&mut self) -> Result<(), anyhow::Error> {
        println!("wait_hello");
        match self.ws_read {
            Some(ref mut read) => {
                let msg = read.next().await;
                // 获得下一条消息，必须是文本类型
                if let Some(Ok(Message::Binary(ref bytes))) = msg {
                    let mut z = ZlibDecoder::new(&bytes[..]);
                    let mut s = String::new();
                    z.read_to_string(&mut s)?;
                    // 反序列化一下
                    let frame: KookWSFrame<HashMap<String, Value>> = serde_json::from_str(&s)?;
                    // 信令位是否为Hello，否则就出错
                    if frame.s == WS_HELLO {
                        // 获取消息中的SN，如果有的话
                        if let Some(sn) = frame.sn {
                            self.sn = sn;
                        }

                        // 如果是WS_HELLO 理论上肯定有数据，这里直接展开
                        let frame_data = frame.d.unwrap();

                        // 获取数据中状态代码
                        let code_data = frame_data.get(&WS_DATA_CODE_FIELD.to_owned());

                        // 获取session_id 代码
                        let session_id_data =
                            frame_data.get(&WS_DATA_HELLO_SESSION_ID_FIELD.to_owned());

                        if let Some(Value::Number(code_number)) = code_data {
                            let code = code_number.as_u64().unwrap();
                            // 状态代码是否正确
                            match code {
                                WS_DATA_CODE_OK => {
                                    // 拿出sessio_id
                                    if let Some(Value::String(ref session_id)) = session_id_data {
                                        self.session_id = Some(session_id.to_owned());
                                        return Ok(());
                                    }
                                }
                                WS_DATA_CODE_MISS_PARAM => {
                                    bail!("缺少参数.")
                                }

                                WS_DATA_CODE_INVAILD_TOKEN => {
                                    bail!("无效token")
                                }

                                WS_DATA_CODE_TOKEN_VALID_FAIL => {
                                    bail!("token 验证失败")
                                }

                                WS_DATA_CODE_TOKEN_EXPIRE => {
                                    bail!("token 过期")
                                }

                                _ => {}
                            }
                        }
                    }
                }
                bail!("不正确的服务器信令: {:?}", &msg)
            }

            None => bail!("没有接受到来自服务器的Hello信令."),
        }
    }

    // 正常来说，永远不会返回
    async fn work(&mut self) -> Result<(), anyhow::Error> {
        println!("work");
        // 心跳定时器，30s一次
        let mut keeplive_interval = tokio::time::interval(Duration::from_secs(30));

        let mut timeout_count = 0;

        let read = self.ws_read.as_mut().unwrap();
        let write = self.ws_write.as_mut().unwrap();

        loop {
            tokio::select! {
                _ = keeplive_interval.tick() =>  {
                    // 每30秒发送一个ping，然后监听pong_rece是否有响应包传来
                    let ping_frame = KookWSFrame::<HashMap<String, Value>>::ping(self.sn);
                    let ping_msg = Message::try_from(ping_frame).unwrap();
                    write.send(ping_msg).await?;
                    match timeout(Duration::from_secs(6), self.heart_channel.1.recv()).await {
                        Err(_) => {
                            // 最多超时6次，六次之后进入超时状态
                            if timeout_count >= 6  {
                                self.state = BotState::HeartTimeout;
                                bail!("pong 超时")
                            }
                        },
                        Ok(_) => timeout_count = 0,
                    };
                }

                message = read.next() => {
                    if let Some(Ok(msg)) = message {
                       let frame =  KookWSFrame::try_from(msg)?;
                    if let Some(sn) = frame.sn {
                        println!("rece sn: {}", sn);
                        // 第一次进入，机器人没有信令，以收到的第一条服务器的信令为准
                        if self.sn == 0 {
                            self.sn = sn
                        }

                        // 小于机器人的信令，说明是处理过的消息，丢弃
                        if self.sn > sn {
                             continue;
                        }

                        // 将信令加入待处理Map
                        self.wait_processing_msg_map.insert(sn, frame);
                    } else {
                        println!("rece no sn msg.");
                        // 没有信令的消息，也是有可能的
                        match self.process_msg_chan.0.send(frame).await {
                            Err(e) => bail!("消息传递错误：{}",e),
                            Ok(_) => {
                                println!("消息已传递");
                            },
                        };
                    }

                    // 处理待处理的信令
                    loop {
                    // 下一条信令编号
                    let next_sn = self.sn + 1;
                    println!("检查sn = {} 是否存在", next_sn);
                    match self.wait_processing_msg_map.remove(&next_sn) {
                        Some(f) => {
                            match self.process_msg_chan.0.send(f).await {
                                Err(e) => bail!("消息传递错误：{}",e),
                                _ => {},
                            };
                            // 更新信令
                            self.sn = next_sn;
                        },
                        // 没有的话直接退出，继续熬
                        None => {
                            break;
                        },
                    }
                }
                };
               }

                frame = self.process_msg_chan.1.recv() => {
                    if let Some(frame) = frame {
                        println!("process msg {:?}", frame);
                         match frame.s {
                            WS_MESSAGE => {
                                // 事件触发, 抽象一个事件钩子出来
                                println!("消息来咯。");
                            },

                            WS_PONG => {
                                println!("pong! live ;)");
                                self.heart_channel.0.send(true).await?;
                            },

                            WS_RECONNECT =>  {
                                println!("当前连接失效，准备重新连接...");
                                self.state = BotState::Reconnect;
                            },

                            WS_RESUME_ACK => {
                                println!("连接已恢复.")
                            },

                            _ => {
                                println!("未接电话：{}", frame.s);
                            }
                           };
                    }
                }
            }
        }
    }
}
