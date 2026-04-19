//! Phase 1 の合成ゴルフコース。
//!
//! 200×40 タイルのグリッドに、ティー・フェアウェイ・ラフ・バンカー・ウォーター・
//! グリーンを手書きで配置する。高さはコーナー単位（201×41）でプリコンピュートし、
//! [`HeightMap`] の continuity contract を自動で満たす。
//!
//! ティー矩形（`x=3..=6, y=18..=21`）は完全に平坦な `20.0m` のルーフトップ。
//! そこから踏み出すと地面はフェアウェイ帯（±0.4m 起伏）に落ちる。バンカーは
//! 窪み、ウォーターは固体（Phase 1 ではハザード判定は描画のみ）、グリーンは
//! ピン (190.5, 20.5) を最低点にした椀。

use rand::Rng;
use rand::SeedableRng;
use rand::rngs::StdRng;
use termray::{CornerHeights, HeightMap, TILE_VOID, TILE_WALL, TileMap, TileType};

/// ティーグラウンド。平坦 `20.0m` のルーフトップとして扱う。
pub const TILE_TEE: TileType = 3;
/// フェアウェイ。base_height の起伏がそのまま出る。
pub const TILE_FAIRWAY: TileType = 4;
/// ラフ。フェアウェイと同じ高さ、描画のみ色替え。
pub const TILE_ROUGH: TileType = 5;
/// バンカー。base_height から 1.5m 窪む。
pub const TILE_BUNKER: TileType = 6;
/// グリーン。ホールを最低点とする椀。
pub const TILE_GREEN: TileType = 7;
/// ウォーターハザード。Phase 1 では solid 扱い。
pub const TILE_WATER: TileType = 8;

const MAP_W: usize = 200;
const MAP_H: usize = 40;
const CORNER_W: usize = MAP_W + 1;
const CORNER_H: usize = MAP_H + 1;

const TEE_X: std::ops::RangeInclusive<i32> = 3..=6;
const TEE_Y: std::ops::RangeInclusive<i32> = 18..=21;
const FAIRWAY_X: std::ops::RangeInclusive<i32> = 10..=179;
const FAIRWAY_Y: std::ops::RangeInclusive<i32> = 15..=24;
const BUNKER1_X: std::ops::RangeInclusive<i32> = 80..=83;
const BUNKER1_Y: std::ops::RangeInclusive<i32> = 19..=21;
const BUNKER2_X: std::ops::RangeInclusive<i32> = 140..=142;
const BUNKER2_Y: std::ops::RangeInclusive<i32> = 17..=20;
const WATER_X: std::ops::RangeInclusive<i32> = 105..=109;
const WATER_Y: std::ops::RangeInclusive<i32> = 17..=21;
const GREEN_X: std::ops::RangeInclusive<i32> = 180..=194;
const GREEN_Y: std::ops::RangeInclusive<i32> = 16..=23;

const TEE_FLOOR_HEIGHT: f64 = 20.0;
const CEILING_OFFSET: f64 = 3.0;
const PIN_X: f64 = 190.5;
const PIN_Y: f64 = 20.5;
const EYE_HEIGHT: f64 = 0.5;

/// Phase 1 の合成コース。
pub struct Course {
    seed: u64,
    tiles: Vec<TileType>,
    corner_floor: Vec<f64>,
    corner_ceil: Vec<f64>,
}

impl Course {
    /// `seed` で高さ場の位相を決めてコースを生成する。同じ seed で呼ぶと
    /// `corner_floor` / `corner_ceil` は完全一致する。
    pub fn generate(seed: u64) -> Self {
        let mut rng = StdRng::seed_from_u64(seed);
        let phase_x: f64 = rng.random_range(0.0..std::f64::consts::TAU);
        let phase_y: f64 = rng.random_range(0.0..std::f64::consts::TAU);
        let amp_scale: f64 = rng.random_range(0.8..1.2);

        let tiles = build_tiles();
        let (corner_floor, corner_ceil) = build_corner_heights(&tiles, phase_x, phase_y, amp_scale);

        Self {
            seed,
            tiles,
            corner_floor,
            corner_ceil,
        }
    }

    /// コース生成に使われた seed。
    pub fn seed(&self) -> u64 {
        self.seed
    }

    /// ピン（ホール）の (x, y) ワールド座標。タイル中心。
    pub fn pin(&self) -> (f64, f64) {
        (PIN_X, PIN_Y)
    }

    /// ティーから打つときのカメラ初期位置 (x, y, z)。
    /// z はルーフトップ高さ + アイレベル。
    pub fn tee_spawn(&self) -> (f64, f64, f64) {
        (5.0, 20.0, TEE_FLOOR_HEIGHT + EYE_HEIGHT)
    }

    /// `(cx, cy)` のタイル種（グリッド座標、i32）。範囲外は `None`。
    pub fn tile_at(&self, cx: i32, cy: i32) -> Option<TileType> {
        if cx < 0 || cy < 0 || (cx as usize) >= MAP_W || (cy as usize) >= MAP_H {
            return None;
        }
        Some(self.tiles[cy as usize * MAP_W + cx as usize])
    }
}

fn build_tiles() -> Vec<TileType> {
    let mut tiles = vec![TILE_ROUGH; MAP_W * MAP_H];
    // 外周壁
    for x in 0..MAP_W {
        tiles[x] = TILE_WALL;
        tiles[(MAP_H - 1) * MAP_W + x] = TILE_WALL;
    }
    for y in 0..MAP_H {
        tiles[y * MAP_W] = TILE_WALL;
        tiles[y * MAP_W + (MAP_W - 1)] = TILE_WALL;
    }
    // フェアウェイ帯
    paint_rect(&mut tiles, &FAIRWAY_X, &FAIRWAY_Y, TILE_FAIRWAY);
    // バンカー
    paint_rect(&mut tiles, &BUNKER1_X, &BUNKER1_Y, TILE_BUNKER);
    paint_rect(&mut tiles, &BUNKER2_X, &BUNKER2_Y, TILE_BUNKER);
    // ウォーター
    paint_rect(&mut tiles, &WATER_X, &WATER_Y, TILE_WATER);
    // グリーン
    paint_rect(&mut tiles, &GREEN_X, &GREEN_Y, TILE_GREEN);
    // ティー（フェアウェイより外側、最後に塗って優先）
    paint_rect(&mut tiles, &TEE_X, &TEE_Y, TILE_TEE);
    tiles
}

fn paint_rect(
    tiles: &mut [TileType],
    xs: &std::ops::RangeInclusive<i32>,
    ys: &std::ops::RangeInclusive<i32>,
    tile: TileType,
) {
    for y in ys.clone() {
        for x in xs.clone() {
            if x < 0 || y < 0 || (x as usize) >= MAP_W || (y as usize) >= MAP_H {
                continue;
            }
            tiles[y as usize * MAP_W + x as usize] = tile;
        }
    }
}

fn tile_of(tiles: &[TileType], cx: i32, cy: i32) -> Option<TileType> {
    if cx < 0 || cy < 0 || (cx as usize) >= MAP_W || (cy as usize) >= MAP_H {
        return None;
    }
    Some(tiles[cy as usize * MAP_W + cx as usize])
}

/// コーナー優先順位。数値が大きいほど優先。壁は最優先で floor=0.0 を強制する。
fn tile_priority(tile: TileType) -> u32 {
    match tile {
        TILE_WALL => 100,
        TILE_TEE => 90,
        TILE_BUNKER => 80,
        TILE_WATER => 70,
        TILE_GREEN => 60,
        TILE_FAIRWAY => 50,
        TILE_ROUGH => 40,
        _ => 10,
    }
}

/// コーナー `(cx, cy)` を代表するタイル種。NW/NE/SW/SE の 4 セルから
/// 優先度最大のものを返す。
fn corner_tile(tiles: &[TileType], cx: i32, cy: i32) -> TileType {
    let candidates = [
        tile_of(tiles, cx - 1, cy - 1),
        tile_of(tiles, cx, cy - 1),
        tile_of(tiles, cx - 1, cy),
        tile_of(tiles, cx, cy),
    ];
    let mut best = TILE_WALL;
    let mut best_prio = 0u32;
    let mut any = false;
    for c in candidates.into_iter().flatten() {
        let p = tile_priority(c);
        if !any || p > best_prio {
            best = c;
            best_prio = p;
            any = true;
        }
    }
    if any { best } else { TILE_WALL }
}

fn base_height(cx: f64, cy: f64, phase_x: f64, phase_y: f64, amp_scale: f64) -> f64 {
    // 低周波 sin 波 2 本で ±0.4m 程度の起伏。
    let hx = (cx * 0.07 + phase_x).sin();
    let hy = (cy * 0.11 + phase_y).sin();
    0.4 * amp_scale * (hx * 0.6 + hy * 0.4)
}

fn corner_floor_at(
    tiles: &[TileType],
    cx: i32,
    cy: i32,
    phase_x: f64,
    phase_y: f64,
    amp_scale: f64,
) -> f64 {
    let tile = corner_tile(tiles, cx, cy);
    let fx = cx as f64;
    let fy = cy as f64;
    match tile {
        TILE_WALL | TILE_VOID => 0.0,
        TILE_TEE => TEE_FLOOR_HEIGHT,
        TILE_BUNKER => base_height(fx, fy, phase_x, phase_y, amp_scale) - 1.5,
        TILE_WATER => -0.2,
        TILE_GREEN => {
            let dx = fx - PIN_X;
            let dy = fy - PIN_Y;
            let d2 = dx * dx + dy * dy;
            base_height(fx, fy, phase_x, phase_y, amp_scale) * 0.3 - 0.3 * (-d2 / 6.0).exp()
        }
        _ => base_height(fx, fy, phase_x, phase_y, amp_scale),
    }
}

fn build_corner_heights(
    tiles: &[TileType],
    phase_x: f64,
    phase_y: f64,
    amp_scale: f64,
) -> (Vec<f64>, Vec<f64>) {
    let mut floor = vec![0.0; CORNER_W * CORNER_H];
    let mut ceil = vec![0.0; CORNER_W * CORNER_H];
    for cy in 0..CORNER_H {
        for cx in 0..CORNER_W {
            let f = corner_floor_at(tiles, cx as i32, cy as i32, phase_x, phase_y, amp_scale);
            floor[cy * CORNER_W + cx] = f;
            ceil[cy * CORNER_W + cx] = f + CEILING_OFFSET;
        }
    }
    (floor, ceil)
}

fn corner_index(cx: i32, cy: i32) -> Option<usize> {
    if cx < 0 || cy < 0 || (cx as usize) >= CORNER_W || (cy as usize) >= CORNER_H {
        return None;
    }
    Some(cy as usize * CORNER_W + cx as usize)
}

impl TileMap for Course {
    fn width(&self) -> usize {
        MAP_W
    }

    fn height(&self) -> usize {
        MAP_H
    }

    fn get(&self, x: i32, y: i32) -> Option<TileType> {
        self.tile_at(x, y)
    }

    fn is_solid(&self, x: i32, y: i32) -> bool {
        match self.tile_at(x, y) {
            None => true,
            Some(TILE_WALL) | Some(TILE_VOID) | Some(TILE_WATER) => true,
            Some(_) => false,
        }
    }
}

impl HeightMap for Course {
    fn cell_heights(&self, x: i32, y: i32) -> CornerHeights {
        let sample = |cx: i32, cy: i32| -> (f64, f64) {
            if let Some(i) = corner_index(cx, cy) {
                (self.corner_floor[i], self.corner_ceil[i])
            } else {
                (0.0, CEILING_OFFSET)
            }
        };
        let (f_nw, c_nw) = sample(x, y);
        let (f_ne, c_ne) = sample(x + 1, y);
        let (f_sw, c_sw) = sample(x, y + 1);
        let (f_se, c_se) = sample(x + 1, y + 1);
        CornerHeights {
            floor: [f_nw, f_ne, f_sw, f_se],
            ceil: [c_nw, c_ne, c_sw, c_se],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_seed_produces_identical_corner_heights() {
        let a = Course::generate(42);
        let b = Course::generate(42);
        assert_eq!(a.corner_floor, b.corner_floor);
        assert_eq!(a.corner_ceil, b.corner_ceil);
    }

    #[test]
    fn different_seed_diverges() {
        let a = Course::generate(42);
        let b = Course::generate(43);
        assert_ne!(a.corner_floor, b.corner_floor);
    }

    #[test]
    fn tee_corners_are_flat_twenty() {
        let c = Course::generate(42);
        // ティー矩形は x=3..=6, y=18..=21。タイル中心でも、タイル内部コーナー
        // (x=4..=6, y=19..=21) でも高さ 20.0 のはず。優先順位ルールが効いて
        // いれば境界コーナー (x=3, y=18 等) もティー扱いで 20.0 になる。
        for cy in 18..=22 {
            for cx in 3..=7 {
                let idx = corner_index(cx, cy).expect("in range");
                let h = c.corner_floor[idx];
                assert!(
                    (h - TEE_FLOOR_HEIGHT).abs() < 1e-9,
                    "tee corner ({cx},{cy}) should be {TEE_FLOOR_HEIGHT}, got {h}"
                );
            }
        }
    }

    #[test]
    fn bunker_lower_than_fairway() {
        let c = Course::generate(42);
        let bunker = c.cell_heights(80, 20).floor[0];
        let fairway = c.cell_heights(60, 20).floor[0];
        assert!(
            bunker < fairway,
            "bunker floor {bunker} should be lower than fairway {fairway}"
        );
    }

    #[test]
    fn borders_are_solid() {
        let c = Course::generate(42);
        assert!(c.is_solid(0, 10));
        assert!(c.is_solid(199, 10));
        assert!(c.is_solid(50, 0));
        assert!(c.is_solid(50, 39));
    }

    #[test]
    fn water_is_solid() {
        let c = Course::generate(42);
        assert!(c.is_solid(107, 19));
    }

    #[test]
    fn fairway_not_solid() {
        let c = Course::generate(42);
        assert!(!c.is_solid(60, 20));
    }

    #[test]
    fn tee_not_solid() {
        let c = Course::generate(42);
        assert!(!c.is_solid(5, 20));
    }

    #[test]
    fn green_not_solid() {
        let c = Course::generate(42);
        assert!(!c.is_solid(185, 20));
    }

    #[test]
    fn out_of_bounds_is_solid() {
        let c = Course::generate(42);
        assert!(c.is_solid(-1, 10));
        assert!(c.is_solid(10, -1));
        assert!(c.is_solid(200, 10));
        assert!(c.is_solid(10, 40));
    }

    #[test]
    fn ceil_tracks_floor_with_offset() {
        let c = Course::generate(42);
        for i in 0..c.corner_floor.len() {
            let d = c.corner_ceil[i] - c.corner_floor[i];
            assert!(
                (d - CEILING_OFFSET).abs() < 1e-9,
                "ceil - floor should be {CEILING_OFFSET}, got {d}"
            );
        }
    }

    #[test]
    fn tee_spawn_matches_rooftop() {
        let c = Course::generate(42);
        let (x, y, z) = c.tee_spawn();
        assert!((x - 5.0).abs() < 1e-9);
        assert!((y - 20.0).abs() < 1e-9);
        assert!((z - (TEE_FLOOR_HEIGHT + EYE_HEIGHT)).abs() < 1e-9);
    }

    #[test]
    fn pin_is_on_green() {
        let c = Course::generate(42);
        let (px, py) = c.pin();
        assert_eq!(c.tile_at(px as i32, py as i32), Some(TILE_GREEN));
    }
}
