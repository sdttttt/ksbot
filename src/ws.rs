use std::collections::HashMap;

use anyhow::bail;
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

// 数据中的状态代码
pub const WS_DATA_CODE_OK: u64 = 0;
pub const WS_DATA_CODE_MISS_PARAM: u64 = 40100;
pub const WS_DATA_CODE_INVAILD_TOKEN: u64 = 40101;
pub const WS_DATA_CODE_TOKEN_VALID_FAIL: u64 = 40102;
pub const WS_DATA_CODE_TOKEN_EXPIRE: u64 = 40103;

// 数据字段
pub const WS_DATA_CODE_FIELD: &str = "code";
pub const WS_DATA_HELLO_SESSION_ID_FIELD: &str = "session_id";

#[derive(Serialize, Deserialize)]
pub struct KookWSFrame {
    pub s: u8,                             // 信令
    pub d: Option<HashMap<String, Value>>, // 数据
    pub sn: Option<u64>,                   // 会话字段，应该用不上
}

impl KookWSFrame {
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
            sn: Some(sn)
        }
    }
}

impl TryFrom<Message> for KookWSFrame {
    type Error = anyhow::Error;

    fn try_from(v: Message) -> Result<Self, Self::Error> {
        match v {
            Message::Text(text) => {
                let frame = serde_json::from_str::<Self>(&text)?;
                Ok(frame)
            }

            _ => bail!("消息类型不是文本类型，这..怎么可能.."),
        }
    }
}

impl TryFrom<KookWSFrame> for Message {
    type Error = anyhow::Error;

    fn try_from(f: KookWSFrame) -> Result<Self, Self::Error> {
        let json_str = serde_json::to_string::<KookWSFrame>(&f)?;
        Ok(Message::Text(json_str))
    }
}
