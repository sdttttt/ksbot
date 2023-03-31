use std::io::Read;

use anyhow::bail;
use flate2::read::ZlibDecoder;
use log::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio_tungstenite::tungstenite::Message;

// 信令
pub const WS_MESSAGE: u8 = 0;
pub const WS_HELLO: u8 = 1;
pub const WS_PING: u8 = 2;
pub const WS_PONG: u8 = 3;
pub const WS_RESUME: u8 = 4;
pub const WS_RECONNECT: u8 = 5;
pub const WS_RESUME_ACK: u8 = 6;

//// 数据中的状态代码
//pub const WS_DATA_CODE_OK: u64 = 0;
//pub const WS_DATA_CODE_MISS_PARAM: u64 = 40100;
//pub const WS_DATA_CODE_INVAILD_TOKEN: u64 = 40101;
//pub const WS_DATA_CODE_TOKEN_VALID_FAIL: u64 = 40102;
//pub const WS_DATA_CODE_TOKEN_EXPIRE: u64 = 40103;

//// 数据字段
//pub const WS_DATA_CODE_FIELD: &str = "code";
//pub const WS_DATA_HELLO_SESSION_ID_FIELD: &str = "session_id";

#[derive(Debug, Serialize, Deserialize)]
pub struct KookWSFrame<T> {
    pub s: u8,           // 信令
    pub d: Option<T>,    // 数据
    pub sn: Option<u64>, // 消息计数ID和服务端保持一致
}

#[derive(Debug, Serialize, Deserialize)]
pub struct KookEventMessage {
    pub id: Option<String>,
    pub channel_name: Option<String>,
    // 内容
    pub content: Option<String>,
    // 随机串，与用户消息发送 api 中传的 nonce 保持一致
    pub nonce: Option<String>,
    //  1:文字消息, 2:图片消息，3:视频消息，4:文件消息， 8:音频消息，9:KMarkdown，10:card 消息，255:系统消息, 其它的暂未开放
    #[serde(rename = "type")]
    pub typ: Option<u64>,
    // 消息发送时间的毫秒时间戳
    pub msg_timestamp: Option<u64>,
    // 频道ID
    pub target_id: Option<String>,
    // 发送人ID
    pub author_id: Option<String>,
    // 消息唯一ID
    pub msg_id: Option<String>,
    // 不同的消息类型，结构不一致
    // extra 这个有两个格式，暂时不做
    pub banner: Option<String>,
    // 是否机器人
    pub bot: Option<bool>,
    // 昵称
    pub nickname: Option<String>,
    // 在线
    pub online: Option<bool>,
}

impl<T> KookWSFrame<T> {
    pub fn ping(sn: u64) -> Self {
        Self {
            s: WS_PING,
            d: None,
            sn: Some(sn),
        }
    }

    pub fn resume(sn: u64) -> Self {
        Self {
            s: WS_RESUME,
            d: None,
            sn: Some(sn),
        }
    }
}

impl<T: for<'a> Deserialize<'a>> TryFrom<Message> for KookWSFrame<T> {
    type Error = anyhow::Error;

    fn try_from(v: Message) -> Result<KookWSFrame<T>, Self::Error> {
        match v {
            Message::Text(text) => {
                trace!("revc: {}", text);
                let frame = serde_json::from_str::<KookWSFrame<T>>(&text)?;
                Ok(frame)
            }

            // 压缩类型
            Message::Binary(bytes) => {
                let mut z = ZlibDecoder::new(&bytes[..]);
                let mut s = String::new();
                z.read_to_string(&mut s)?;
                debug!("revc: {}", s);
                let frame = serde_json::from_str::<KookWSFrame<T>>(&s)?;
                Ok(frame)
            }

            Message::Ping(_) | Message::Pong(_) => {
                debug!("native ping/pong");
                Ok(KookWSFrame {
                    ..Default::default()
                })
            }

            Message::Close(_) => {
                bail!("要关闭连接了");
            }

            _ => unreachable!(),
        }
    }
}

impl<T: Serialize> TryFrom<KookWSFrame<T>> for Message {
    type Error = anyhow::Error;

    fn try_from(f: KookWSFrame<T>) -> Result<Self, Self::Error> {
        let json_str = serde_json::to_string::<KookWSFrame<T>>(&f)?;
        debug!("send: {}", json_str);
        Ok(Message::Text(json_str))
    }
}

impl TryFrom<KookWSFrame<Value>> for KookWSFrame<KookEventMessage> {
    type Error = anyhow::Error;

    fn try_from(value: KookWSFrame<Value>) -> Result<KookWSFrame<KookEventMessage>, Self::Error> {
        let kem = serde_json::from_value::<KookEventMessage>(value.d.unwrap())?;
        Ok(KookWSFrame {
            sn: value.sn,
            s: value.s,
            d: Some(kem),
        })
    }
}

impl<T> Default for KookWSFrame<T> {
    fn default() -> Self {
        Self {
            s: Default::default(),
            d: Default::default(),
            sn: Default::default(),
        }
    }
}
