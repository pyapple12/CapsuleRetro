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

use crate::monitor::profile::MemoryMap;

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
    /// 写入被白名单拒绝：目标区间未完整落在 memory_map 登记条目内——
    /// 任意地址写在库层即被挡下（PL007 金手指纪律闸）。
    #[error("写入被拒绝: {segment:#06x}:{offset:#x} 起 {len} 字节未落在任何登记条目区间内")]
    WriteDenied {
        /// 写入起始段地址。
        segment: u16,
        /// 写入起始段内偏移。
        offset: u32,
        /// 写入字节数。
        len: usize,
    },
    /// 档案中不存在请求的条目名（触网前即拒绝）。
    #[error("档案中无此条目: {0}")]
    UnknownField(String),
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
    /// CPU 完整寄存器组（上游响应可能省略部分字段，缺省兜 0——容错白名单
    /// 已登记的既定契约）。
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

    /// 写入模拟器内存（白名单闸内，PL007 金手指原语的落点）：目标区间必须
    /// 完整落在 `map` 登记条目内，否则 [`MonitorError::WriteDenied`]——
    /// fork 端点保持通用薄层，纪律收敛在本库一处。
    ///
    /// # 参数
    /// - `map`：memory_map 档案（白名单数据源）
    /// - `segment`：实模式段地址
    /// - `offset`：段内偏移
    /// - `bytes`：待写字节（须完整落在单条条目区间内）
    ///
    /// # Errors
    /// 白名单拒绝返回 [`MonitorError::WriteDenied`]；传输失败或非 2xx
    /// （上游对越界/非法输入返回 400）返回 [`MonitorError`]。
    pub fn write_memory(
        &self,
        map: &MemoryMap,
        segment: u16,
        offset: u32,
        bytes: &[u8],
    ) -> Result<(), MonitorError> {
        if !whitelist_allows(map, segment, offset, bytes.len()) {
            return Err(MonitorError::WriteDenied {
                segment,
                offset,
                len: bytes.len(),
            });
        }
        self.put_memory_raw(segment, offset, bytes)
    }

    /// 按档案条目做读-改-写（金手指）：GET 当前值 → 饱和加法（按条目类型
    /// 封顶防回绕）→ 白名单校验 → 写回。
    ///
    /// # 参数
    /// - `map`：memory_map 档案
    /// - `name`：条目名（如 "gold"）
    /// - `delta`：增量（可为负）
    ///
    /// # 返回
    /// 封顶后的新值。
    ///
    /// # Errors
    /// 未知条目名返回 [`MonitorError::UnknownField`]（触网前拒绝）；读取/
    /// 解码/写入失败返回 [`MonitorError`]。
    pub fn adjust_field(
        &self,
        map: &MemoryMap,
        name: &str,
        delta: i64,
    ) -> Result<i64, MonitorError> {
        let entry = map
            .entries
            .iter()
            .find(|e| e.name == name)
            .ok_or_else(|| MonitorError::UnknownField(name.to_string()))?;
        let current_bytes = self.memory(Some(entry.segment), entry.offset, u32::from(entry.len))?;
        let current = entry.value_type.decode(&current_bytes)?;
        let new_bytes = entry.value_type.encode(current.saturating_add(delta));
        self.write_memory(map, entry.segment, entry.offset, &new_bytes)?;
        entry.value_type.decode(&new_bytes).map_err(Into::into)
    }

    /// 裸 PUT 传输（上游 webserver octet-stream 形态，无业务校验——仅供
    /// [`Self::write_memory`] 白名单闸调用）。
    fn put_memory_raw(&self, segment: u16, offset: u32, bytes: &[u8]) -> Result<(), MonitorError> {
        let url = format!("{}/api/v1/memory/{segment}/{offset:#x}", self.base_url);
        match self
            .agent
            .request("PUT", &url)
            .set("Content-Type", "application/octet-stream")
            .send_bytes(bytes)
        {
            Ok(_) => Ok(()),
            Err(ureq::Error::Status(code, _)) => Err(MonitorError::BadStatus(code)),
            Err(err) => Err(MonitorError::Http(Box::new(err))),
        }
    }
}

/// 白名单判定：写入区间 [offset, offset+len) 须完整落在同段某一登记条目的
/// 覆盖区间内。区间运算用 u64 防 offset+len 回绕；len 为 0 直接拒绝（空写
/// 无意义且只会制造歧义）。
fn whitelist_allows(map: &MemoryMap, segment: u16, offset: u32, len: usize) -> bool {
    if len == 0 {
        return false;
    }
    let start = u64::from(offset);
    let end = start + len as u64;
    map.entries.iter().any(|entry| {
        entry.segment == segment
            && start >= u64::from(entry.offset)
            && end <= u64::from(entry.offset) + u64::from(entry.len)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::monitor::profile::MemoryMap;

    /// 独立结构样例：仓库真实 memory_map.json（109 条登记条目）——验证
    /// 样例取自真实数据，禁止手写与实现同源的 mock 自证。
    fn real_map() -> MemoryMap {
        let path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../retro/memory_map.json");
        MemoryMap::load(&path).expect("仓库档案应可解析")
    }

    /// PL007.2 验证：白名单只放行完整落在登记条目区间内的写入——条目
    /// 自身命中、右界内最后一字节命中放行；右界越出、同段空档、异段地址
    /// 一律拒绝。
    #[test]
    fn 白名单只放行登记条目区间() {
        let map = real_map();
        // gold = 0x5AA3:0x000E u32le：条目自身区间命中
        assert!(whitelist_allows(&map, 0x5AA3, 0x000E, 4));
        // 右界内最后一字节（0x11..0x12）命中
        assert!(whitelist_allows(&map, 0x5AA3, 0x0011, 1));
        // 右界越出 1 字节（0xE..0x13）→ 拒绝
        assert!(!whitelist_allows(&map, 0x5AA3, 0x000E, 5));
        // 同段空档（0x5AA3 段仅登记 gold 一条）→ 拒绝
        assert!(!whitelist_allows(&map, 0x5AA3, 0x0040, 2));
        // 异段套用他条目偏移（0x5AA4 段 bank_gems 止于 0xE，其后无登记）→ 拒绝
        assert!(!whitelist_allows(&map, 0x5AA4, 0x000E, 4));
    }

    /// PL007.2 验证：未知条目名在触网前即拒绝（查找先于任何 HTTP 请求，
    /// 测试中无 dosbox 在场也能稳定断言）。
    #[test]
    fn 未知条目名在触网前即拒绝() {
        let client = RestClient::new(18086);
        let err = client
            .adjust_field(&real_map(), "not_a_field", 10_000)
            .expect_err("未知条目应报错");
        assert!(
            matches!(err, MonitorError::UnknownField(ref name) if name == "not_a_field"),
            "{err}"
        );
    }
}
