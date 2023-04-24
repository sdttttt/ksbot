use crate::api;
use crate::conf::{BotConfig, BOT_STORE_FILE_PATH};
use crate::network_frame::{
    KookEventMessage, KookWSFrame, WS_HELLO, WS_MESSAGE, WS_PONG, WS_RECONNECT, WS_RESUME_ACK,
};
use crate::utils::ExponentRegress;
use anyhow::bail;
use futures_util::{
    stream::{SplitSink, SplitStream, StreamExt},
    SinkExt,
};
use serde::{Deserialize, Serialize};

use tracing::{debug, error, info, warn};

use serde_json::Value;
use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap};
use std::fs::File;
use std::io::{Read, Seek, Write};
use std::time::Duration;
use tokio::sync::broadcast;
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::time::timeout;
use tokio::{net::TcpStream, time::sleep};
use tokio_tungstenite::{connect_async, tungstenite::Message, MaybeTlsStream, WebSocketStream};

#[derive(Debug, Clone)]
pub enum BotNetworkEvent {
    Connect(),
    Heart(),
    Error(),
    Shutdown(),
    Message(Box<KookEventMessage>),
}

// 保存在硬盘上的数据
#[derive(Debug, Serialize, Deserialize)]
pub struct BotStore {
    session_id: String,
    sn: u64,
    gateway: String,
}

enum BotState {
    GetGateway,     // 初始化：尝试获取ws网关
    ConnectGateway, // 已经获取网关：尝试连接上网关
    Working,        // 工作中
    HeartTimeout,   // 心跳超时：恢复到ConnectGateway
    Reconnect,      // 重新连接：恢复到GetGateway
    Resume,         // 恢复连接：一般是机器人重启
}

// 机器人运行时，基础设施
pub struct BotNetworkRuntime {
    state: BotState,
    conf: BotConfig,
    store_f: File,

    // 网关
    gateway_url: Option<String>,
    // 恢复用
    gateway_resume_url: Option<String>,

    // 会话ID
    session_id: Option<String>,
    // 消息ID
    sn: u64,

    /// WebSocket  connection
    ws_write: Option<SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, Message>>,
    ws_read: Option<SplitStream<WebSocketStream<MaybeTlsStream<TcpStream>>>>,

    // 待处理的事件消息: Key = 消息的SN
    // Reverse 是为了倒序，默认排序是sn最小在前, 这样pop反而是最大的sn
    wait_process_event_map: BinaryHeap<Reverse<KookWSFrame<Value>>>,
    // 广播事件发送
    event_sender: Option<broadcast::Sender<BotNetworkEvent>>,
    /// 心跳信道
    heart_chan: (Sender<bool>, Receiver<bool>),
}

impl BotNetworkRuntime {
    pub fn init(conf: BotConfig) -> BotNetworkRuntime {
        // 以可读可写打开可创建的方式打开机器人持久化文件
        let mut f = std::fs::File::options()
            .read(true)
            .write(true)
            .create(true)
            .open(&conf.store_path)
            .expect("文件(创建/打开)出错");
        let mut f_content = "".to_owned();
        f.read_to_string(&mut f_content)
            .expect("读取机器人持久化文件错误");

        let mut session_id: Option<String> = None;
        let mut sn = 0;
        let mut gateway_url: Option<String> = None;
        let mut state = BotState::GetGateway;
        if f_content.trim().is_empty() == false {
            let store = serde_json::from_str::<BotStore>(&f_content).unwrap_or_else(|_| panic!("序列化机器人持久化文件错误, 如果反复出现该错误可删除该文件。(默认的持久化文件名：{})", BOT_STORE_FILE_PATH));
            // 从文件中读取
            session_id = Some(store.session_id);
            sn = store.sn;
            gateway_url = Some(store.gateway);
            state = BotState::Resume;
        }

        Self {
            state,
            store_f: f,
            conf,
            gateway_url,
            gateway_resume_url: None,
            session_id,
            sn,
            ws_read: None,
            ws_write: None,
            event_sender: None,
            wait_process_event_map: BinaryHeap::with_capacity(64),
            heart_chan: tokio::sync::mpsc::channel::<bool>(1),
        }
    }

    // 订阅该网络事件，可多次订阅
    pub fn subscribe_event(&mut self) -> broadcast::Receiver<BotNetworkEvent> {
        if let Some(sender) = &self.event_sender {
            return sender.subscribe();
        }
        let (sender, receiver) = broadcast::channel(64);
        self.event_sender = Some(sender);
        receiver
    }

    pub async fn connect(&mut self) -> Result<(), anyhow::Error> {
        // 网关连接计数器
        let mut connect_gateway_count = 0;
        let eg = ExponentRegress::from_base(2);
        loop {
            match self.state {
                BotState::GetGateway | BotState::Reconnect => match self.try_get_gateway().await {
                    Ok(_) => {
                        self.state = BotState::ConnectGateway;
                        info!("获取网关成功，尝试连接...")
                    }
                    Err(e) => {
                        warn!("获取网关失败: {}", e);
                        sleep(Duration::from_secs(4)).await;
                    }
                },

                BotState::Resume => {
                    self.state = BotState::ConnectGateway;
                    warn!("尝试恢复连接...");
                    self.try_get_resume_gateway().await?;
                }

                BotState::ConnectGateway | BotState::HeartTimeout => {
                    match self.connect_wss_server().await {
                        Ok(_) => {
                            self.state = BotState::Working;
                            // 成功后，重置连接网关计数
                            connect_gateway_count = 0;
                            eg.reset();
                            info!("连接网关成功，等待服务器响应.")
                        }
                        Err(e) => {
                            error!("连接网关失败: {}", e);
                            if connect_gateway_count > 2 {
                                error!("连接网关已到最大重试次数, 重新获取网关.");
                                self.state = BotState::GetGateway;
                            }
                            connect_gateway_count += 1;
                            sleep(Duration::from_secs(eg.get() as u64)).await;
                        }
                    }
                }

                BotState::Working => {
                    if let Err(e) = self.work().await {
                        error!("工作出错: {:?}", e)
                    }

                    self.state = BotState::GetGateway;
                }
            };
        }
    }

    // 获取网关
    async fn try_get_gateway(&mut self) -> Result<(), anyhow::Error> {
        info!("try_get_gateway");
        let wss_url = api::http::get_wss_gateway().await?;
        self.gateway_url = Some(wss_url);
        Ok(())
    }

    async fn try_get_resume_gateway(&mut self) -> Result<(), anyhow::Error> {
        info!("try_resume");
        let wss_url = format!(
            "{}&resume=1&sn={}&session_id={}",
            self.gateway_url.to_owned().unwrap(),
            self.sn,
            self.session_id.to_owned().unwrap()
        );

        self.gateway_resume_url = Some(wss_url);
        Ok(())
    }

    // 连接到WS网关
    async fn connect_wss_server(&mut self) -> Result<(), anyhow::Error> {
        info!("connect_wss_server");
        // 如果当前状态是Resume, 网关优先使用gateway_resume_url
        let gateway_url = {
            if let Some(resume) = self.gateway_resume_url.to_owned() {
                resume
            } else {
                self.gateway_url.to_owned().unwrap()
            }
        };
        info!("gateway_url: {}", gateway_url);
        let ws_url = url::Url::parse(&gateway_url)?;
        let (wconn, _) = connect_async(ws_url).await?;
        let (write, read) = wconn.split();

        self.ws_read = Some(read);
        self.ws_write = Some(write);

        if let Some(sender) = &self.event_sender {
            if let Err(e) = sender.send(BotNetworkEvent::Connect()) {
                error!("通信运行时消息发送失败：{}", e);
            }
        }
        Ok(())
    }

    // 正常来说，永远不会返回
    async fn work(&mut self) -> Result<(), anyhow::Error> {
        // 心跳定时器，30s一次
        let mut keeplive_interval = tokio::time::interval(Duration::from_secs(30));

        let eg = ExponentRegress::from_base(2);
        // 快进1轮，从4开始
        eg.forward(1);

        // 机器人持久化定时器，10s一次
        let mut store_sync_interval = tokio::time::interval(Duration::from_secs(10));

        let mut timeout_count = 0;

        let read = self.ws_read.as_mut().unwrap();
        let write = self.ws_write.as_mut().unwrap();

        loop {
            tokio::select! {
                // 心跳
                _ = keeplive_interval.tick() =>  {
                    // 每30秒发送一个ping，然后监听pong_rece是否有响应包传来
                    let ping_frame = KookWSFrame::<HashMap<String, Value>>::ping(self.sn);
                    let ping_msg = Message::try_from(ping_frame).unwrap();
                    write.send(ping_msg).await?;
                    info!("client -> ping -> server");
                    match timeout(Duration::from_secs(eg.get() as u64), self.heart_chan.1.recv()).await {
                        Err(_) => {
                            // 最多超时3次，3次之后进入超时状态
                            if timeout_count >= 3  {
                                self.state = BotState::HeartTimeout;
                                bail!("pong 超时")
                            }
                        },
                        Ok(_) => {
                            timeout_count = 0;
                            eg.reset();
                            eg.forward(1);
                        }
                    };
                }

                // WS消息接收
                message = read.next() => {
                    if let Some(Ok(msg)) = message {
                        // 解析数据帧
                       let frame = match  KookWSFrame::<Value>::try_from(msg) {
                            Ok(f) => {
                                // 无效的消息
                                if f.s == u8::default()
                                && f.d == Default::default()
                                && f.sn == Default::default() {
                                    debug!("无效的消息");
                                    continue;
                                }
                                f
                            },
                            Err(e) => {
                                error!("{}", e);
                                bail!("消息帧解析：{}", e);
                            },
                       };

                    match frame.sn {

                        // 处理不带信令的数据帧
                        None => {
                            match frame.s {
                                WS_PONG => {
                                    info!("client <- pong <- server ");
                                    self.heart_chan.0.send(true).await?;
                                    if let Some(sender) = &self.event_sender {
                                        if let Err(e) = sender.send(BotNetworkEvent::Heart()) {
                                            error!("通信运行时消息发送失败：{}", e);
                                        }
                                    }
                                },

                                WS_RECONNECT =>  {
                                    error!("当前连接失效，准备重新连接...");
                                    // KookDocs: 任何时候，收到 reconnect 包，应该将当前消息队列，sn等全部清空，然后回到第 1 步，否则可能会有消息错乱等各种问题。
                                    self.sn = 0;
                                    self.session_id = None;
                                    self.gateway_resume_url = None;
                                    self.wait_process_event_map.clear();
                                    self.state = BotState::Reconnect;
                                },

                                WS_RESUME_ACK | WS_HELLO => {
                                    info!("client <- hello/resume_ack <- server");
                                    let v = frame.d.unwrap();
                                    if let Value::String(ref session_id) = v["session_id"] {
                                        self.session_id = Some(session_id.to_owned());
                                        info!("会话: {}", session_id);
                                    }
                                },

                                _ => {
                                    info!("未接电话：{:?}", frame);
                                }
                            };
                        }

                        // 根据官方文档，只有事件帧会有SN。
                        // 带SN的数据帧都装进wait_processing_msg_map等待后续处理
                       Some(sn) => {
                            info!("rece sn: {}", sn);
                            // 小于机器人的信令，说明是处理过的消息，丢弃
                            if self.sn >= sn {
                                continue;
                            }

                            // 将信令加入待处理Map
                            if self.sn < sn {
                                self.wait_process_event_map.push(Reverse(frame));
                            }
                        }
                    }


                    // 处理待处理的信令
                    loop {
                        // 下一条信令编号
                        let next_sn = self.sn + 1;

                        match self.wait_process_event_map.peek() {
                            Some(Reverse(f))  => {
                                if f.sn !=  next_sn.into() {
                                    info!("待处理的消息数量: {}",self.wait_process_event_map.len());
                                    break;
                                }
                            },
                            None => {
                                info!("消息已处理完成");
                                break
                            },
                        }

                        if let Some(Reverse(f)) = self.wait_process_event_map.pop() {
                            match f.s {
                                WS_MESSAGE => {
                                        // 重新对帧进行序列化，变成事件消息格式
                                        let event_frame = KookWSFrame::<KookEventMessage>::try_from(f).expect("事件消息反序列化失败.");
                                        // 发送消息广播给所有的信道
                                        if let Some(sender) = &self.event_sender {
                                            if let Err(e) = sender.send(BotNetworkEvent::Message(Box::new(event_frame.d.unwrap()))) {
                                                error!("通信运行时消息发送失败：{}", e);
                                            }
                                        }
                                },
                                _ => {
                                    warn!("未接电话：{:?}", f);
                                }
                            };
                            self.sn = next_sn;
                        }
                    }
                };
            }

                // 持久化机器人状态
                _ = store_sync_interval.tick() =>  {
                    let store = BotStore::new(
                        self.session_id.to_owned().unwrap_or_else(|| "".to_owned()),
                        self.sn,
                        self.gateway_url.to_owned().unwrap_or_else(|| "".to_owned())
                    );
                    match serde_json::to_string(&store) {
                        Err(e) => bail!("写入出错：{}", e),
                        Ok(v) => {
                            let _ = self.store_f.rewind();
                            if let Err(e) = self.store_f.write_all(v.as_bytes()) {
                                bail!("写入出错：{}", e);
                            }
                            self.store_f.sync_data()?;
                        }
                    }

                }
            }
        }
    }
}

impl BotStore {
    fn new(session_id: String, sn: u64, gateway: String) -> Self {
        Self {
            session_id,
            sn,
            gateway,
        }
    }
}
