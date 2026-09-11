//! game.json 适配层配置聚合（`retro/game.json` 的类型化读取）。
//!
//! 设计理由：launcher 与 packer 共用同一份配置结构，字段校验集中在此；
//! 严格抛错、无默认值兜底（AGENTS.md 错误策略），缺字段/非法值一律报错。
//! 关联：字段结构由 PL002 阶段 B 定义（dosbox/assets/paths 三节）。

use std::path::{Path, PathBuf};

use serde::Deserialize;
use thiserror::Error;

/// game.json 读取与校验错误。
#[derive(Debug, Error)]
pub enum GameConfigError {
    /// 文件读取失败。
    #[error("game.json 读取失败 {path}: {source}")]
    Io {
        /// 失败的文件路径。
        path: PathBuf,
        /// 底层 IO 错误。
        source: std::io::Error,
    },
    /// JSON 解析失败或结构不符（含缺字段）。
    #[error("game.json 解析失败: {0}")]
    Json(#[from] serde_json::Error),
    /// 端口不在合法范围（1024–65535）。
    #[error("非法 webserver 端口 {port}：须在 1024–65535")]
    InvalidPort {
        /// 非法的端口值。
        port: u16,
    },
    /// cpu_cycles 必须为正数。
    #[error("非法 cpu_cycles {cycles}：须为正数")]
    InvalidCycles {
        /// 非法的 cycles 值。
        cycles: u32,
    },
    /// name 为空或含非法字符（name 是运行时目录与启动命令的来源，须可安全落盘）。
    #[error("非法游戏名 {name:?}：须非空且只含字母、数字、下划线、连字符")]
    InvalidName {
        /// 非法的 name 值。
        name: String,
    },
    /// save_files 清单含空白条目或路径分隔符（存档收集只认工作副本顶层文件名）。
    #[error("非法 save_files 清单: {0}")]
    InvalidSaveFiles(String),
    /// paths.game_dir 为空（原版释放源缺失配置，FIX002.15 专用变体）。
    #[error("paths.game_dir 为空（game.json: {0}）")]
    InvalidGameDir(String),
    /// patches 定义非法（id 为空、重复或 patch 路径为空）。
    #[error("非法 patches 定义: {0}")]
    InvalidPatches(String),
}

/// 适配层聚合配置（retro/game.json 的类型化形态）。
#[derive(Debug, Deserialize)]
pub struct GameConfig {
    /// 游戏名（适配层约定：同时是 C: 盘内启动命令的小写来源，如 XEEN → xeen）
    pub name: String,
    /// 游戏版本（对外展示用）
    pub version: String,
    /// DOSBox 运行配置节
    pub dosbox: DosboxSection,
    /// 适配层资源节
    pub assets: AssetsSection,
    /// 运行路径节
    pub paths: PathsSection,
    /// 补丁定义列表（hack 版 = 原版 + 运行时应用补丁；可为空数组）
    pub patches: Vec<PatchRef>,
}

/// DOSBox 运行配置节。
#[derive(Debug, Deserialize)]
pub struct DosboxSection {
    /// DOSBox 可执行文件路径（仓库根相对）
    pub exe_path: PathBuf,
    /// webserver 端口（1024–65535）
    pub webserver_port: u16,
    /// CPU cycles（正数）
    pub cpu_cycles: u32,
}

/// 适配层资源节。
#[derive(Debug, Deserialize)]
pub struct AssetsSection {
    /// 内存档案路径（PL001 档案）
    pub memory_map: PathBuf,
    /// 攻略文档路径
    pub guide: PathBuf,
    /// 封面图路径（可选，2:3 卡带封面；缺省打包告警、待机页回落占位——PL006.2）
    pub cover: Option<PathBuf>,
    /// 16:9 主视觉路径（可选；缺省不入包——PL006.2，供后续 theme 使用）
    pub hero: Option<PathBuf>,
    /// 队伍头像 PNG 目录（可选；PL009.2——目录内 charNN.png 按面容文件号编号，
    /// HUD 头像取图源；缺省不入包，HUD 回落槽位号占位）
    pub faces_dir: Option<PathBuf>,
}

/// 运行路径节。
#[derive(Debug, Deserialize)]
pub struct PathsSection {
    /// 原版游戏目录（相对 game.json 所在目录，如 "XEEN" → retro/XEEN/，只读释放源）
    pub game_dir: PathBuf,
    /// guest 游戏可执行文件名（可选；缺省回落 name 小写派生——exe 基名与
    /// name 不一致的游戏显式声明，FIX002.25）
    pub guest_exe: Option<String>,
    /// 存档文件清单（工作副本顶层文件名通配符，仅支持 `*`；save 回写收集依据）
    pub save_files: Vec<String>,
}

/// 补丁定义（game.json patches[] 条目）。
#[derive(Debug, Deserialize)]
pub struct PatchRef {
    /// 补丁 id（save meta.hack 引用此 id，如 "test-hack"）
    pub id: String,
    /// 展示名（UI 待机页切换项）
    pub name: String,
    /// 补丁包 zip 路径（相对 game.json 所在目录）
    pub patch: PathBuf,
}

impl std::str::FromStr for GameConfig {
    type Err = GameConfigError;

    /// # Errors
    /// JSON 非法或字段校验失败时返回 [`GameConfigError`]。
    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let config: Self = serde_json::from_str(text)?;
        config.validate()?;
        Ok(config)
    }
}

impl GameConfig {
    /// 从文件加载并校验。
    ///
    /// # 参数
    /// - `path`：game.json 路径
    ///
    /// # Errors
    /// 文件缺失、JSON 非法或字段校验失败时返回 [`GameConfigError`]。
    pub fn load(path: &Path) -> Result<Self, GameConfigError> {
        let text = std::fs::read_to_string(path).map_err(|source| GameConfigError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        text.parse::<Self>()
    }

    /// 字段校验：name 合法性、端口范围与 cycles 正数（缺字段由 serde 报错覆盖）。
    fn validate(&self) -> Result<(), GameConfigError> {
        let name_ok = !self.name.is_empty()
            && self
                .name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
        if !name_ok {
            return Err(GameConfigError::InvalidName {
                name: self.name.clone(),
            });
        }
        if !(1024..=65535).contains(&self.dosbox.webserver_port) {
            return Err(GameConfigError::InvalidPort {
                port: self.dosbox.webserver_port,
            });
        }
        if self.dosbox.cpu_cycles == 0 {
            return Err(GameConfigError::InvalidCycles {
                cycles: self.dosbox.cpu_cycles,
            });
        }
        if self.paths.game_dir.as_os_str().is_empty() {
            return Err(GameConfigError::InvalidGameDir(self.name.clone()));
        }
        if let Some(bad) = self
            .paths
            .save_files
            .iter()
            .find(|p| p.trim().is_empty() || p.contains('/') || p.contains('\\'))
        {
            return Err(GameConfigError::InvalidSaveFiles(format!(
                "{bad:?}（须为非空顶层文件名通配符，仅支持 *）"
            )));
        }
        let mut ids: Vec<&str> = Vec::new();
        for patch in &self.patches {
            if patch.id.trim().is_empty() || patch.patch.as_os_str().is_empty() {
                return Err(GameConfigError::InvalidPatches(format!(
                    "id/patch 为空: {:?}",
                    patch.id
                )));
            }
            if ids.contains(&patch.id.as_str()) {
                return Err(GameConfigError::InvalidPatches(format!(
                    "id 重复: {:?}",
                    patch.id
                )));
            }
            ids.push(&patch.id);
        }
        Ok(())
    }

    /// guest 游戏可执行文件名：显式 `paths.guest_exe` 优先，缺省回落 name
    /// 小写派生（适配层约定 exe 基名 = name 小写，如 XEEN → xeen.exe；
    /// 消费方：launcher 完整性断言与 gameconf 启动命令）。
    pub fn guest_exe_name(&self) -> String {
        self.paths
            .guest_exe
            .clone()
            .unwrap_or_else(|| format!("{}.exe", self.name.to_ascii_lowercase()))
    }
}
