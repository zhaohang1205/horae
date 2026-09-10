//! 飞书免公网 IP WebSocket 长连接客户端。
//!
//! 采用纯 Rust 同步阻塞 WebSocket (`tungstenite` + `rustls`) 实现，
//! 支持自动获取长连接网关 Endpoint、心跳保活 (Ping-Pong)、数据帧解析分发、以及断线指数退避重连。

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use prost::Message as ProstMessage;
use rusqlite::Connection;
use tungstenite::Message;

use super::auth::FeishuAuth;
use super::handler::handle_data_frame;
use super::proto::{Frame, METHOD_DATA};

/// 飞书长连接客户端
pub struct FeishuWsClient {
    auth: Arc<FeishuAuth>,
    stop: Arc<AtomicBool>,
}

fn set_socket_read_timeout(
    socket: &mut tungstenite::WebSocket<tungstenite::stream::MaybeTlsStream<std::net::TcpStream>>,
    timeout: Option<Duration>,
) {
    let res = match socket.get_mut() {
        tungstenite::stream::MaybeTlsStream::Plain(s) => s.set_read_timeout(timeout),
        tungstenite::stream::MaybeTlsStream::Rustls(s) => s.get_mut().set_read_timeout(timeout),
        _ => Ok(()),
    };
    if let Err(e) = res {
        eprintln!("[feishu-ws] 设置 Socket 读取超时失败: {e:#}");
    }
}

impl FeishuWsClient {
    pub fn new(auth: Arc<FeishuAuth>) -> Self {
        Self {
            auth,
            stop: Arc::new(AtomicBool::new(false)),
        }
    }

    /// 获取停止标志的句柄
    pub fn stop_handle(&self) -> Arc<AtomicBool> {
        self.stop.clone()
    }

    /// 请求终止长连接循环
    pub fn stop(&self) {
        self.stop.store(true, Ordering::Relaxed);
    }

    /// 执行长连接主事件循环（阻塞当前线程，适用于后台工作线程）。
    /// 遭遇网络波动时自动进行指数退避重连，直到 `stop` 标志置位。
    pub fn run(&self, conn: &Connection) {
        let mut retry_delay = Duration::from_secs(2);
        let max_retry_delay = Duration::from_secs(60);

        while !self.stop.load(Ordering::Relaxed) {
            eprintln!("[feishu-ws] 正在获取长连接网关地址...");
            let (endpoint_url, cfg) = match self.auth.fetch_ws_endpoint() {
                Ok(res) => {
                    retry_delay = Duration::from_secs(2); // 成功重置退避
                    res
                }
                Err(e) => {
                    eprintln!(
                        "[feishu-ws] 获取长连接网关失败: {e:#}，将在 {:?} 后重试",
                        retry_delay
                    );
                    thread::sleep(retry_delay);
                    retry_delay = (retry_delay * 2).min(max_retry_delay);
                    continue;
                }
            };

            eprintln!("[feishu-ws] 正在建立 WebSocket 长连接通道...");
            let url = match url::Url::parse(&endpoint_url) {
                Ok(u) => u,
                Err(e) => {
                    eprintln!("[feishu-ws] 网关 URL 格式非法: {e:#}");
                    thread::sleep(retry_delay);
                    continue;
                }
            };

            // 从 URL query 中提取专属 service_id (用于构造 Ping 心跳包)
            let service_id: i32 = url
                .query_pairs()
                .find(|(k, _)| k == "service_id")
                .and_then(|(_, v)| v.parse().ok())
                .unwrap_or(0);

            let ping_interval_secs = cfg
                .map(|c| c.reconnect_interval.clamp(15, 60))
                .unwrap_or(30);

            let (mut socket, _response) = match tungstenite::connect(url) {
                Ok((socket, response)) => {
                    eprintln!(
                        "[feishu-ws] 长连接建立成功 (HTTP {})，开始监听事件",
                        response.status()
                    );
                    (socket, response)
                }
                Err(e) => {
                    eprintln!(
                        "[feishu-ws] 建立 WebSocket 连接失败: {e:#}，将在 {:?} 后重试",
                        retry_delay
                    );
                    thread::sleep(retry_delay);
                    retry_delay = (retry_delay * 2).min(max_retry_delay);
                    continue;
                }
            };

            // 设置底层 TCP 读超时为 5 秒，防止永久阻塞无法发送客户端保活 Ping 或响应终止信号
            set_socket_read_timeout(&mut socket, Some(Duration::from_secs(5)));
            let mut last_ping = std::time::Instant::now();

            // 进入单次连接的数据接收与保活循环
            while !self.stop.load(Ordering::Relaxed) {
                // 客户端周期性主动向飞书网关发送 Ping 心跳包保活
                if last_ping.elapsed() >= Duration::from_secs(ping_interval_secs) {
                    let ping = Frame::build_ping(service_id);
                    let ping_bytes = ping.encode_to_vec();
                    if let Err(e) = socket.send(Message::Binary(ping_bytes.into())) {
                        eprintln!("[feishu-ws] 发送 Ping 保活失败: {e:#}");
                        break;
                    }
                    last_ping = std::time::Instant::now();
                }

                let msg = match socket.read() {
                    Ok(m) => m,
                    Err(tungstenite::Error::Io(ref e))
                        if e.kind() == std::io::ErrorKind::WouldBlock
                            || e.kind() == std::io::ErrorKind::TimedOut =>
                    {
                        continue;
                    }
                    Err(e) => {
                        eprintln!("[feishu-ws] 长连接读取异常或断开: {e:#}");
                        break;
                    }
                };

                match msg {
                    Message::Binary(bin) => {
                        let frame = match Frame::decode(&bin[..]) {
                            Ok(f) => f,
                            Err(e) => {
                                eprintln!("[feishu-ws] 解码 Protobuf Frame 失败: {e:#}");
                                continue;
                            }
                        };

                        // 1. 网关 Ping 心跳包响应
                        if frame.is_ping() {
                            let pong = Frame::build_pong(frame.seq_id, frame.service);
                            let pong_bytes = pong.encode_to_vec();
                            if let Err(e) = socket.send(Message::Binary(pong_bytes.into())) {
                                eprintln!("[feishu-ws] 发送 Pong 响应失败: {e:#}");
                                break;
                            }
                            continue;
                        }

                        // 2. 业务数据帧
                        if frame.method == METHOD_DATA {
                            match handle_data_frame(conn, &self.auth, &frame) {
                                Ok(Some(ack_frame)) => {
                                    let ack_bytes = ack_frame.encode_to_vec();
                                    if let Err(e) = socket.send(Message::Binary(ack_bytes.into())) {
                                        eprintln!("[feishu-ws] 回发 ACK 失败: {e:#}");
                                        break;
                                    }
                                }
                                Ok(None) => {}
                                Err(e) => {
                                    eprintln!("[feishu-ws] 处理事件业务失败: {e:#}");
                                }
                            }
                        }
                    }
                    Message::Ping(payload) => {
                        if let Err(e) = socket.send(Message::Pong(payload)) {
                            eprintln!("[feishu-ws] 响应底层 Pong 失败: {e:#}");
                            break;
                        }
                    }
                    Message::Close(_) => {
                        eprintln!("[feishu-ws] 收到服务端 Close 帧，连接断开");
                        break;
                    }
                    _ => {}
                }
            }

            // 若未收到终止信号，稍后重连
            if !self.stop.load(Ordering::Relaxed) {
                eprintln!("[feishu-ws] 连接已中断，将在 3 秒后尝试重连...");
                thread::sleep(Duration::from_secs(3));
            }
        }

        eprintln!("[feishu-ws] 长连接监听线程已退出");
    }
}
