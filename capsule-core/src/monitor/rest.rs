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
    /// 内存读取短读：响应体字节数少于请求的 `len`（不做静默截断）。
    #[error("内存读取不足: 请求 {expected} 字节，实际 {actual} 字节")]
    ShortRead { expected: usize, actual: usize },
    /// 观察条目处理失败（附条目名，便于多条目轮询时定位问题条目）。
    #[error("条目 {name}: {source}")]
    Entry {
        /// 失败的档案条目名。
        name: String,
        /// 底层失败原因（读取或解码）。
        source: Box<MonitorError>,
    },
}

/// CPU 寄存器快照（仅保留实模式常用的通用寄存器与段寄存器）。
///
/// 真实 API 会省略部分字段（实测响应中无 `es`），因此整结构体允许缺字段。
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Registers {
    /// 代码段寄存器。
    pub cs: u32,
    /// 数据段寄存器。
    pub ds: u32,
    /// 附加段寄存器（实测响应可能缺省，serde default 兜 0）。
    pub es: u32,
    /// 栈段寄存器。
    pub ss: u32,
    /// FS 段寄存器。
    pub fs: u32,
    /// GS 段寄存器。
    pub gs: u32,
    /// EAX 累加器。
    pub eax: u32,
    /// EBX 基址寄存器。
    pub ebx: u32,
    /// ECX 计数寄存器。
    pub ecx: u32,
    /// EDX 数据寄存器。
    pub edx: u32,
    /// ESI 源变址寄存器。
    pub esi: u32,
    /// EDI 目的变址寄存器。
    pub edi: u32,
    /// EBP 栈帧基址指针。
    pub ebp: u32,
    /// ESP 栈指针。
    pub esp: u32,
    /// EIP 指令指针。
    pub eip: u32,
    /// EFLAGS 标志寄存器。
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
    /// 恰好 `len` 字节的原始内存；响应不足 `len` 时返回
    /// [`MonitorError::ShortRead`]（短读污染快照会让差分报误导性的
    /// LengthMismatch，因此严格报错而非静默截断）。
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
        if buf.len() != len as usize {
            return Err(MonitorError::ShortRead {
                expected: len as usize,
                actual: buf.len(),
            });
        }
        Ok(buf)
    }

    /// 读取实模式全底层内存（1 MiB 快照），差分搜索的原料。
    pub fn snapshot_lower_memory(&self) -> Result<Vec<u8>, MonitorError> {
        self.memory(None, 0, 0x10_0000)
    }
}
