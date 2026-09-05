//! rest：DOSBox Staging HTTP REST 客户端。
//!
//! 封装 DOSBox Staging 0.83 内置 webserver（`[webserver]`，仅绑 127.0.0.1）的
//! 只读端点：cpu/state 与 memory 读取。设计理由：外部观察组件与模拟器之间唯一
//! 的数据通道，集中处理超时、状态码与错误映射；不做重试（严格抛错主线），轮询
//! 层的瞬时失败策略由调用方（watch）负责。

use std::io::Read as _;
use std::time::Duration;

use serde::Deserialize;
use thiserror::Error;

/// REST 调用过程中的全部错误。
#[derive(Debug, Error)]
pub enum MonitorError {
    /// 传输层失败（连接拒绝、超时、TLS 等）。装箱以保持错误枚举体积小。
    #[error("HTTP 请求失败: {0}")]
    Http(Box<ureq::Error>),
    /// 服务端返回了非 2xx 状态码。
    #[error("非预期的 HTTP 状态码: {0}")]
    BadStatus(u16),
    /// 读取响应体失败。
    #[error("读取响应失败: {0}")]
    Io(#[from] std::io::Error),
    /// 响应体不是合法 JSON 或结构与预期不符。
    #[error("响应 JSON 解析失败: {0}")]
    Json(#[from] serde_json::Error),
    /// 档案条目解码失败（宽度不符等）。
    #[error("档案解码失败: {0}")]
    Profile(#[from] crate::monitor::profile::ProfileError),
    /// 差分的两份快照长度不一致（快照必须等长）。
    #[error("快照长度不一致: {prev} 字节 vs {curr} 字节")]
    LengthMismatch { prev: usize, curr: usize },
}

/// CPU 寄存器快照（仅保留实模式常用的通用寄存器与段寄存器）。
///
/// 真实 API 会省略部分字段（实测响应中无 `es`），因此整结构体允许缺字段。
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Registers {
    pub cs: u32,
    pub ds: u32,
    pub es: u32,
    pub ss: u32,
    pub fs: u32,
    pub gs: u32,
    pub eax: u32,
    pub ebx: u32,
    pub ecx: u32,
    pub edx: u32,
    pub esi: u32,
    pub edi: u32,
    pub ebp: u32,
    pub esp: u32,
    pub eip: u32,
    pub flags: u32,
}

/// `GET /api/v1/cpu/state` 的响应结构。
#[derive(Debug, Clone, Default, Deserialize)]
pub struct CpuState {
    #[serde(default)]
    pub registers: Registers,
}

/// DOSBox Staging REST 客户端（只读端点封装）。
pub struct RestClient {
    agent: ureq::Agent,
    base_url: String,
}

impl RestClient {
    /// 创建指向本机 DOSBox webserver 的客户端。
    ///
    /// # 参数
    /// - `port`：webserver 端口（DOSBox 默认 8086）
    ///
    /// # 备注
    /// 请求超时固定 5 秒：慢响应按错误上抛，避免轮询被无限挂起。
    pub fn new(port: u16) -> Self {
        let agent = ureq::AgentBuilder::new()
            .timeout(Duration::from_secs(5))
            .build();
        Self {
            agent,
            base_url: format!("http://127.0.0.1:{port}"),
        }
    }

    /// 发起 GET 并把非 2xx 归一化为 [`MonitorError::BadStatus`]。
    fn get(&self, path: &str) -> Result<ureq::Response, MonitorError> {
        let url = format!("{}{path}", self.base_url);
        match self.agent.get(&url).call() {
            Ok(resp) => Ok(resp),
            Err(ureq::Error::Status(code, _)) => Err(MonitorError::BadStatus(code)),
            Err(err) => Err(MonitorError::Http(Box::new(err))),
        }
    }

    /// 读取 CPU 寄存器快照。
    ///
    /// # 返回
    /// [`CpuState`]；网络/状态码/JSON 任一失败时返回 [`MonitorError`]。
    pub fn cpu_state(&self) -> Result<CpuState, MonitorError> {
        Ok(self.get("/api/v1/cpu/state")?.into_json::<CpuState>()?)
    }

    /// 读取模拟器内存。
    ///
    /// # 参数
    /// - `segment`：实模式段地址；`None` 时使用一参形式的物理地址路径
    /// - `offset`：段内偏移（segment 为 None 时为物理地址）
    /// - `len`：读取字节数
    ///
    /// # 返回
    /// 恰好 `len` 字节的原始内存（响应不足时按实际读到的返回）。
    pub fn memory(
        &self,
        segment: Option<u16>,
        offset: u32,
        len: u32,
    ) -> Result<Vec<u8>, MonitorError> {
        let path = match segment {
            Some(seg) => format!("/api/v1/memory/{seg}/{offset:#x}/{len:#x}"),
            None => format!("/api/v1/memory/{offset:#x}/{len:#x}"),
        };
        let mut buf = Vec::with_capacity(len as usize);
        self.get(&path)?
            .into_reader()
            .take(u64::from(len))
            .read_to_end(&mut buf)?;
        Ok(buf)
    }

    /// 读取实模式全底层内存（1 MiB 快照），差分搜索的原料。
    pub fn snapshot_lower_memory(&self) -> Result<Vec<u8>, MonitorError> {
        self.memory(None, 0, 0x10_0000)
    }
}
