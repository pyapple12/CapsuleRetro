//! maze：XEEN 迷宫数据（MAZExxxx.DAT，892 字节定长）解析（PL010）。
//!
//! 格式双源一致（OpenXeen `WOXMazeFile.loadMaze` 逐段 + Xeen Wiki），并在本仓
//! DARK.CUR / XEEN.CUR 全部主图上锚点实测（偏移 768 的 u16 == 文件名地图号，
//! 311/311 通过，见 z.plan 附录 PL010）：
//! `[0..512)` WallData 256×u16（16×16 格，每格 4 个半字节，低 → 高）→
//! `[512..768)` CellFlag 每格 1 字节 → `[768..770)` 内嵌地图号 →
//! `[770..778)` SurrMazes 北/东/南/西 → `[778..780)` flags1（b14 可休息 /
//! b15 可存盘）→ `[780..782)` flags2（b14 黑暗 / b15 户外）→
//! `[782..798)` 墙 LUT → `[798..814)` 地表 LUT → 尾段杂项 + seen/stepped。
//! 数据来源注意：CUR 容器数据区为明文（不做 XOR，与 .CC 不同），由调用方
//! 经 [`crate::cc_archive::CcArchive::read_raw`] 取字节。命名规则
//! `MAZE%04d.DAT`（id ≥ 100 时 `MAZEX%03d.DAT`）；16 位哈希空间撞车催生的
//! `AAZExxxx.TXT/.HED` 为事件文字/事件头，与本模块无关。

use thiserror::Error;

/// 单边格数（16×16）。
pub const MAP_SIZE: usize = 16;
/// 每图格数。
pub const TILES: usize = MAP_SIZE * MAP_SIZE;
/// 迷宫数据定长（字节）。
pub const MAZE_LEN: usize = 892;

/// 迷宫数据解析错误。
#[derive(Debug, Error)]
pub enum MazeError {
    /// 数据尺寸不是定长 892 字节。
    #[error("迷宫数据尺寸非法: {size} 字节（应为 892）")]
    BadSize {
        /// 实际收到的字节数。
        size: usize,
    },
}

/// 一张迷宫的全部静态数据。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MazeMap {
    /// 文件名中的地图号（调用方传入，与内嵌号核对；变体文件两者不同）。
    pub file_id: u16,
    /// 数据内嵌地图号（偏移 768；2xx/3xx 变体文件回指基图号）。
    pub id_field: u16,
    /// 每格 4 个半字节（低 → 高）：户外语义 = 地表/中层/顶层/覆盖层，
    /// 户内语义 = 朝北/东/南/西墙值；下标 = x + y × 16（数据序，(0,0) 左下）。
    pub tiles: [[u8; 4]; TILES],
    /// 格标志（水域/事件格等，逐字节原值）。
    pub cell_flags: [u8; TILES],
    /// 四向邻接地图号（北/东/南/西；0 = 无邻接）。
    pub surr_nesw: [u16; 4],
    /// 可休息（flags1 b14）。
    pub can_rest: bool,
    /// 可存盘（flags1 b15）。
    pub can_save: bool,
    /// 黑暗图（flags2 b14）。
    pub is_dark: bool,
    /// 户外图（flags2 b15）。
    pub is_outdoors: bool,
    /// 墙类型查找表（16 项，渲染时间接索引）。
    pub wall_lut: [u8; 16],
    /// 地表类型查找表（16 项，渲染时间接索引）。
    pub surface_lut: [u8; 16],
}

impl MazeMap {
    /// 解析 892 字节迷宫数据；尺寸不符严格报错（无容错）。
    pub fn parse(file_id: u16, data: &[u8]) -> Result<Self, MazeError> {
        if data.len() != MAZE_LEN {
            return Err(MazeError::BadSize { size: data.len() });
        }
        let u16_at = |off: usize| u16::from_le_bytes([data[off], data[off + 1]]);
        let mut tiles = [[0u8; 4]; TILES];
        for (t, tile) in tiles.iter_mut().enumerate() {
            let v = u16_at(t * 2);
            *tile = [
                (v & 0xF) as u8,
                ((v >> 4) & 0xF) as u8,
                ((v >> 8) & 0xF) as u8,
                ((v >> 12) & 0xF) as u8,
            ];
        }
        let mut cell_flags = [0u8; TILES];
        cell_flags.copy_from_slice(&data[512..768]);
        let mut surr_nesw = [0u16; 4];
        for (k, s) in surr_nesw.iter_mut().enumerate() {
            *s = u16_at(770 + k * 2);
        }
        let flags1 = u16_at(778);
        let flags2 = u16_at(780);
        let mut wall_lut = [0u8; 16];
        wall_lut.copy_from_slice(&data[782..798]);
        let mut surface_lut = [0u8; 16];
        surface_lut.copy_from_slice(&data[798..814]);
        Ok(Self {
            file_id,
            id_field: u16_at(768),
            tiles,
            cell_flags,
            surr_nesw,
            can_rest: flags1 & 0x4000 != 0,
            can_save: flags1 & 0x8000 != 0,
            is_dark: flags2 & 0x4000 != 0,
            is_outdoors: flags2 & 0x8000 != 0,
            wall_lut,
            surface_lut,
        })
    }
}

/// 迷宫资源名：`MAZE%04d.DAT`；id ≥ 100 时百位换成 X（`MAZEX100.DAT`）。
///
/// # Panics
/// id 为 0 或 ≥ 1000（上游拼名规则即 1–999）。
pub fn maze_file_name(mid: u16) -> String {
    assert!((1..1000).contains(&mid), "地图号越界: {mid}");
    let mut s = format!("{mid:04}");
    if mid >= 100 {
        s.replace_range(0..1, "X");
    }
    format!("MAZE{s}.DAT")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 夹具：按 OpenXeen `loadMaze` 的布局常量逐段构造（规格来源独立于实现）。
    fn fixture(id_field: u16) -> Vec<u8> {
        let mut d = vec![0u8; MAZE_LEN];
        // WallData：格 t 的 u16 = t（半字节即 t 自身各位）
        for t in 0..TILES {
            d[t * 2..t * 2 + 2].copy_from_slice(&(t as u16).to_le_bytes());
        }
        // 已知复合值：0x12 格 = 0x1234 → 半字节 [4,3,2,1]
        d[0x24] = 0x34;
        d[0x25] = 0x12;
        // CellFlag：逐格 = 下标
        for (i, b) in d[512..768].iter_mut().enumerate() {
            *b = i as u8;
        }
        d[768..770].copy_from_slice(&id_field.to_le_bytes());
        for (k, v) in [11u16, 12, 13, 14].iter().enumerate() {
            d[770 + k * 2..772 + k * 2].copy_from_slice(&v.to_le_bytes());
        }
        d[778..780].copy_from_slice(&0xC000u16.to_le_bytes()); // 可休息 + 可存盘
        d[780..782].copy_from_slice(&0xC000u16.to_le_bytes()); // 黑暗 + 户外
        for (i, b) in d[782..798].iter_mut().enumerate() {
            *b = i as u8;
        }
        for (i, b) in d[798..814].iter_mut().enumerate() {
            *b = (i + 16) as u8;
        }
        d
    }

    #[test]
    fn 解析字段全对() {
        let m = MazeMap::parse(8, &fixture(8)).expect("合法数据应解析成功");
        assert_eq!(m.file_id, 8);
        assert_eq!(m.id_field, 8);
        assert_eq!(m.tiles[0], [0, 0, 0, 0]);
        assert_eq!(m.tiles[0x12], [4, 3, 2, 1]);
        assert_eq!(m.tiles[0xFF], [0xF, 0xF, 0, 0]);
        assert_eq!(m.cell_flags[7], 7);
        assert_eq!(m.surr_nesw, [11, 12, 13, 14]);
        assert!(m.can_rest && m.can_save && m.is_dark && m.is_outdoors);
        assert_eq!(m.wall_lut[..4], [0, 1, 2, 3]);
        assert_eq!(m.surface_lut[..4], [16, 17, 18, 19]);
    }

    #[test]
    fn 变体文件保留双号() {
        // 2xx/3xx 变体：文件号 200，内嵌基图号 104
        let m = MazeMap::parse(200, &fixture(104)).expect("变体数据同样应解析成功");
        assert_eq!(m.file_id, 200);
        assert_eq!(m.id_field, 104);
    }

    #[test]
    fn 尺寸不符严格报错() {
        let full = fixture(8);
        for bad in [0usize, 891, 893, 1024] {
            let data = if bad <= full.len() {
                full[..bad.min(full.len())].to_vec()
            } else {
                let mut d = full.clone();
                d.resize(bad, 0);
                d
            };
            let err = MazeMap::parse(8, &data).expect_err("非 892 字节应报错");
            assert!(matches!(err, MazeError::BadSize { .. }), "{err}");
        }
    }

    #[test]
    fn 命名规则() {
        assert_eq!(maze_file_name(1), "MAZE0001.DAT");
        assert_eq!(maze_file_name(8), "MAZE0008.DAT");
        assert_eq!(maze_file_name(99), "MAZE0099.DAT");
        assert_eq!(maze_file_name(100), "MAZEX100.DAT");
        assert_eq!(maze_file_name(142), "MAZEX142.DAT");
        assert_eq!(maze_file_name(999), "MAZEX999.DAT");
    }

    #[test]
    #[should_panic(expected = "地图号越界")]
    fn 命名越界panic() {
        maze_file_name(0);
    }
}
