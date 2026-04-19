//! 軸分離の衝突判定ヘルパ。
//!
//! カメラやボールが 2D マップ上を動くときに使う、X 軸と Y 軸それぞれの
//! 1 ステップ進行関数。`radius` は進行方向の probe オフセットで、
//! ちょうど壁を抜けて角に引っかからないようにするためのもの。

use termray::TileMap;

/// `dx` 方向に 1 ステップ進めた後の新しい X 座標と、壁にぶつかったかを返す。
///
/// - `dx == 0.0` の場合は即座に `(x, false)` を返す。
/// - 進行先のタイル `(probe_x.floor(), y.floor())` が `is_solid` なら移動せず
///   `(x, true)` を返す（呼び出し側は速度を 0 にする想定）。
pub fn step_x<M: TileMap>(map: &M, x: f64, y: f64, dx: f64, radius: f64) -> (f64, bool) {
    if dx == 0.0 {
        return (x, false);
    }
    let nx = x + dx;
    let probe_x = nx + dx.signum() * radius;
    if map.is_solid(probe_x.floor() as i32, y.floor() as i32) {
        (x, true)
    } else {
        (nx, false)
    }
}

/// `dy` 方向に 1 ステップ進めた後の新しい Y 座標と、壁にぶつかったかを返す。
/// [`step_x`] と対称。
pub fn step_y<M: TileMap>(map: &M, x: f64, y: f64, dy: f64, radius: f64) -> (f64, bool) {
    if dy == 0.0 {
        return (y, false);
    }
    let ny = y + dy;
    let probe_y = ny + dy.signum() * radius;
    if map.is_solid(x.floor() as i32, probe_y.floor() as i32) {
        (y, true)
    } else {
        (ny, false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Course;

    #[test]
    fn step_x_blocks_against_solid() {
        let c = Course::generate(42);
        // (0, 20) は外周壁で solid。x=1.5 から左に動こうとすると probe_x が x=0 領域に入り blocked。
        let (nx, blocked) = step_x(&c, 1.5, 20.0, -0.8, 0.2);
        assert!(blocked, "movement toward border wall should be blocked");
        assert!((nx - 1.5).abs() < 1e-9, "x must not advance when blocked");
    }

    #[test]
    fn step_x_passes_through_empty() {
        let c = Course::generate(42);
        // フェアウェイ内 (60, 20) 周辺は solid ではない。
        let (nx, blocked) = step_x(&c, 60.0, 20.0, 0.3, 0.2);
        assert!(!blocked, "movement inside fairway should not be blocked");
        assert!(
            (nx - 60.3).abs() < 1e-9,
            "x must advance by dx when clear, got {nx}"
        );
    }

    #[test]
    fn step_y_blocks_against_solid() {
        let c = Course::generate(42);
        // y=1.5 から上に動くと y=0 の外周壁に probe がぶつかる。
        let (ny, blocked) = step_y(&c, 60.0, 1.5, -0.8, 0.2);
        assert!(blocked, "movement toward border wall should be blocked");
        assert!((ny - 1.5).abs() < 1e-9, "y must not advance when blocked");
    }

    #[test]
    fn step_y_passes_through_empty() {
        let c = Course::generate(42);
        let (ny, blocked) = step_y(&c, 60.0, 20.0, 0.3, 0.2);
        assert!(!blocked, "movement inside fairway should not be blocked");
        assert!(
            (ny - 20.3).abs() < 1e-9,
            "y must advance by dy when clear, got {ny}"
        );
    }

    #[test]
    fn step_x_zero_is_noop() {
        let c = Course::generate(42);
        let (nx, blocked) = step_x(&c, 60.0, 20.0, 0.0, 0.2);
        assert!(!blocked);
        assert!((nx - 60.0).abs() < 1e-9);
    }

    #[test]
    fn step_y_zero_is_noop() {
        let c = Course::generate(42);
        let (ny, blocked) = step_y(&c, 60.0, 20.0, 0.0, 0.2);
        assert!(!blocked);
        assert!((ny - 20.0).abs() < 1e-9);
    }
}
