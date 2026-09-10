//! 飞书 WebSocket 长连接 Protobuf 二进制帧协议定义。
//!
//! 飞书开放平台长连接网关采用 Protobuf 编码二进制数据帧。
//! 本模块采用 pure Rust 的 `prost` derive 纯手写定义，无需依赖 `protoc` 外部编译器或 `prost-build`。

use prost::Message;

/// 帧头部键值对
#[derive(Clone, PartialEq, Message)]
pub struct Header {
    #[prost(string, tag = "1")]
    pub key: String,
    #[prost(string, tag = "2")]
    pub value: String,
}

impl Header {
    pub fn new(key: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            value: value.into(),
        }
    }
}

/// 飞书长连接二进制交互单元 Frame
#[derive(Clone, PartialEq, Message)]
pub struct Frame {
    #[prost(uint64, tag = "1")]
    pub seq_id: u64,
    #[prost(uint64, tag = "2")]
    pub log_id: u64,
    #[prost(int32, tag = "3")]
    pub service: i32,
    #[prost(int32, tag = "4")]
    pub method: i32,
    #[prost(message, repeated, tag = "5")]
    pub headers: Vec<Header>,
    #[prost(string, optional, tag = "6")]
    pub payload_encoding: Option<String>,
    #[prost(string, optional, tag = "7")]
    pub payload_type: Option<String>,
    #[prost(bytes = "vec", optional, tag = "8")]
    pub payload: Option<Vec<u8>>,
    #[prost(string, optional, tag = "9")]
    pub log_id_new: Option<String>,
}

/// 帧方法类型（与飞书官方网关一致）
pub const METHOD_CONTROL: i32 = 0;
pub const METHOD_DATA: i32 = 1;

/// 消息类型常量
pub const MSG_TYPE_PING: &str = "ping";
pub const MSG_TYPE_PONG: &str = "pong";
pub const MSG_TYPE_EVENT: &str = "event";
pub const MSG_TYPE_ACK: &str = "ack";

impl Frame {
    /// 获取指定 header 键的值
    pub fn header(&self, key: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|h| h.key.eq_ignore_ascii_case(key))
            .map(|h| h.value.as_str())
    }

    /// 判断当前帧是否为网关 Ping 心跳帧
    pub fn is_ping(&self) -> bool {
        self.method == METHOD_CONTROL && self.header("type") == Some(MSG_TYPE_PING)
    }

    /// 构造客户端主动发送的 Ping 心跳保活帧（用于维持长连接在线状态）
    pub fn build_ping(service_id: i32) -> Self {
        Self {
            seq_id: 0,
            log_id: 0,
            service: service_id,
            method: METHOD_CONTROL,
            headers: vec![Header::new("type", MSG_TYPE_PING)],
            payload_encoding: None,
            payload_type: None,
            payload: None,
            log_id_new: None,
        }
    }

    /// 构造网关 Pong 心跳响应帧
    pub fn build_pong(seq_id: u64, service: i32) -> Self {
        Self {
            seq_id,
            log_id: 0,
            service,
            method: METHOD_CONTROL,
            headers: vec![Header::new("type", MSG_TYPE_PONG)],
            payload_encoding: None,
            payload_type: None,
            payload: None,
            log_id_new: None,
        }
    }

    /// 构造事件消费成功或卡片回调的 ACK 响应帧
    pub fn build_ack(seq_id: u64, service: i32, method: i32, ack_payload: Option<Vec<u8>>) -> Self {
        Self {
            seq_id,
            log_id: 0,
            service,
            method,
            headers: vec![Header::new("type", MSG_TYPE_ACK)],
            payload_encoding: None,
            payload_type: None,
            payload: ack_payload,
            log_id_new: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_frame_encode_decode() {
        let frame = Frame {
            seq_id: 12345,
            log_id: 67890,
            service: 1,
            method: METHOD_DATA,
            headers: vec![
                Header::new("type", "event"),
                Header::new("message_id", "msg-001"),
            ],
            payload_encoding: None,
            payload_type: Some("json".into()),
            payload: Some(b"{\"hello\":\"world\"}".to_vec()),
            log_id_new: None,
        };

        let encoded = frame.encode_to_vec();
        assert!(!encoded.is_empty());

        let decoded = Frame::decode(&encoded[..]).expect("decode frame");
        assert_eq!(decoded.seq_id, 12345);
        assert_eq!(decoded.header("type"), Some("event"));
        assert_eq!(decoded.header("message_id"), Some("msg-001"));
        assert_eq!(decoded.header("non_existent"), None);
        assert_eq!(
            decoded.payload.as_deref(),
            Some(b"{\"hello\":\"world\"}".as_slice())
        );
    }

    #[test]
    fn test_ping_pong_frames() {
        let ping = Frame {
            seq_id: 888,
            log_id: 0,
            service: 1,
            method: METHOD_CONTROL,
            headers: vec![Header::new("type", "ping")],
            payload_encoding: None,
            payload_type: None,
            payload: None,
            log_id_new: None,
        };
        assert!(ping.is_ping());

        let pong = Frame::build_pong(ping.seq_id, ping.service);
        assert_eq!(pong.seq_id, 888);
        assert_eq!(pong.method, METHOD_CONTROL);
        assert_eq!(pong.header("type"), Some("pong"));
    }
}
