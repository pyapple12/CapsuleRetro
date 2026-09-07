//! RestClient 的 stub 集成测试（PL001.4/PL001.5，对应验证方案 V2）。
//!
//! 用本地 TcpListener 起假 REST 服务，返回预录的真实 DOSBox 响应
//! （2026-09-05 对 v0.83.0 实测抓取），断言解析与错误传播行为。

use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread;

use capsule_core::monitor::rest::{MonitorError, RestClient};

/// 预录：GET /api/v1/cpu/state 的真实响应体（XEEN 运行中实测）。
const CPU_STATE_JSON: &str = r#"{
  "registers": {
    "cs": 4518, "ds": 36370, "eax": 2052, "ebp": 0, "ebx": 3,
    "ecx": 0, "edi": 32384, "edx": 986, "eip": 25905,
    "esi": 64003, "esp": 3948, "flags": 29254, "fs": 0,
    "gs": 0, "ss": 8572
  }
}"#;

/// memory 端点的固定字节模式（8 字节循环填充，前 8 字节与既有断言一致）。
const MEMORY_PATTERN: &[u8] = &[0x39, 0x05, 0xAB, 0xCD, 0x00, 0xFF, 0x11, 0x22];

/// 启动 stub 服务：按请求路径返回预录响应，返回其端口。
fn spawn_stub() -> u16 {
    spawn_stub_with(None)
}

/// 启动带模式的 stub 服务。
///
/// # 参数
/// - `body_len_override`：Some(n) 时 memory 端点固定返回 n 字节（模拟短读，
///   n 小于请求长度触发客户端 ShortRead）；None 时按请求长度循环填充
///   [`MEMORY_PATTERN`]。
fn spawn_stub_with(body_len_override: Option<usize>) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind stub");
    let port = listener.local_addr().expect("addr").port();
    thread::spawn(move || {
        for stream in listener.incoming() {
            let mut stream = match stream {
                Ok(s) => s,
                Err(_) => break,
            };
            let mut buf = Vec::new();
            let mut chunk = [0u8; 1024];
            loop {
                match stream.read(&mut chunk) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        buf.extend_from_slice(&chunk[..n]);
                        if buf.windows(4).any(|w| w == b"\r\n\r\n") {
                            break;
                        }
                    }
                }
            }
            let request = String::from_utf8_lossy(&buf);
            let path = request.split_whitespace().nth(1).unwrap_or("").to_string();
            let (status, body) = match path.as_str() {
                "/api/v1/cpu/state" => ("200 OK", CPU_STATE_JSON.as_bytes().to_vec()),
                p if p.contains("0xdead") => ("404 Not Found", b"not found".to_vec()),
                p if p.starts_with("/api/v1/memory/") => {
                    let len = p
                        .rsplit('/')
                        .next()
                        .and_then(|raw| u32::from_str_radix(raw.trim_start_matches("0x"), 16).ok())
                        .unwrap_or(MEMORY_PATTERN.len() as u32)
                        as usize;
                    let fill = match body_len_override {
                        Some(n) => n,
                        None => len,
                    };
                    let body: Vec<u8> = (0..fill)
                        .map(|i| MEMORY_PATTERN[i % MEMORY_PATTERN.len()])
                        .collect();
                    ("200 OK", body)
                }
                _ => ("404 Not Found", b"not found".to_vec()),
            };
            let response = format!(
                "HTTP/1.1 {status}\r\nContent-Type: application/json\r\n\
                 Content-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            let _ = stream.write_all(response.as_bytes());
            let _ = stream.write_all(&body);
        }
    });
    port
}

#[test]
fn cpu_state_parses_registers() {
    let client = RestClient::new(spawn_stub());
    let state = client.cpu_state().expect("cpu_state 应成功");
    assert_eq!(state.registers.cs, 4518);
    assert_eq!(state.registers.eip, 25905);
    assert_eq!(state.registers.ss, 8572);
}

#[test]
fn memory_returns_exact_bytes() {
    let client = RestClient::new(spawn_stub());
    let bytes = client.memory(Some(4518), 0x200, 8).expect("memory 应成功");
    assert_eq!(bytes, vec![0x39, 0x05, 0xAB, 0xCD, 0x00, 0xFF, 0x11, 0x22]);
}

#[test]
fn snapshot_reads_requested_length() {
    let client = RestClient::new(spawn_stub());
    let snap = client.snapshot_lower_memory().expect("快照应成功");
    // stub 按请求长度回填，1MB 快照必须足额返回（短读走 ShortRead 严格报错）
    assert_eq!(snap.len(), 0x10_0000);
}

/// 短读（响应体少于请求长度）必须报 ShortRead，不得静默截断（FIX001.14）。
#[test]
fn short_read_returns_error() {
    let client = RestClient::new(spawn_stub_with(Some(4)));
    let err = client.memory(Some(4518), 0x200, 8).unwrap_err();
    assert!(matches!(
        err,
        MonitorError::ShortRead {
            expected: 8,
            actual: 4
        }
    ));
}

#[test]
fn not_found_maps_to_bad_status() {
    let client = RestClient::new(spawn_stub());
    // /api/v1/nonexistent 会命中 stub 的 404 分支
    let err = client.memory(None, 0xDEAD, 4).unwrap_err();
    assert!(matches!(err, MonitorError::BadStatus(404)));
}

#[test]
fn connection_refused_maps_to_http_error() {
    // 占住一个端口再释放，用它构造"必然连接拒绝"的场景
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("addr").port();
    drop(listener);
    let client = RestClient::new(port);
    let err = client.cpu_state().unwrap_err();
    assert!(matches!(err, MonitorError::Http(_)));
}
