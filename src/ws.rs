use std::collections::HashMap;

use serde::Deserialize;
use serde_json::Value;

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

#[derive(Deserialize)]
pub struct KookWSFrame {
    pub s: u8,                             // 信令
    pub d: Option<HashMap<String, Value>>, // 数据
    pub sn: Option<u64>,                   // 会话字段，应该用不上
}
