//! Phase 4 のラウンド進行管理。
//!
//! 1 ホールのプレイ全体を `ParSelect → Playing → Finished` の 3 ステートで
//! 追いかけ、打数・ペナルティ・ストローク履歴・最終結果を保持する。
//! ホールアウト判定はカップ半径と速度上限の 2 条件でスキムオーバーを防ぐ。
//! 純粋ロジックでユニットテストだけでカバーできる。

use termray::TileType;

use crate::BallState;
use crate::shot::Club;

/// カップの半径 (m)。R&A/USGA 規格はカップ直径 4.25 inch = 10.8 cm。
pub const CUP_RADIUS_M: f64 = 0.054;

/// ホールイン判定に使う速度上限 (m/s)。これより速いとカップを弾いて通過
/// するためホールインにしない（"skim over" 対策）。
pub const HOLE_OUT_SPEED_LIMIT: f64 = 2.0;

/// ラウンドのフェーズ。
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum RoundPhase {
    /// Par を選ぶ初期画面。3 / 4 / 5 キーで [`RoundPhase::Playing`] に進む。
    ParSelect,
    /// プレイ中。ショットループが回る。
    Playing,
    /// ホールアウト後。Y でリトライ、N / Esc で終了。
    Finished,
}

/// 1 ストロークの記録。
#[derive(Clone, Debug)]
pub struct StrokeRecord {
    /// 1 始まりのストローク番号。ペナルティは別カウント。
    pub index: u32,
    pub club: Club,
    pub power: f64,
    pub start_pos: [f64; 3],
    pub end_pos: [f64; 3],
    pub rest_tile: Option<TileType>,
    pub penalty: bool,
}

/// ラウンド終了時の集計。
#[derive(Clone, Debug)]
pub struct RoundResult {
    pub par: u32,
    pub strokes: u32,
    pub penalties: u32,
    pub total: u32,
    pub vs_par: i32,
    pub label: &'static str,
}

/// ラウンド全体の状態。
pub struct RoundState {
    pub phase: RoundPhase,
    pub par: u32,
    pub strokes: u32,
    pub penalties: u32,
    pub stroke_log: Vec<StrokeRecord>,
    pub holed_out: bool,
    /// 次のショットの発射位置（= 現在のボール静止位置）。水ペナルティで
    /// 戻す先としても使う。
    pub last_start_pos: [f64; 3],
    pub result: Option<RoundResult>,
}

impl RoundState {
    /// ティー位置を渡して初期化する。Par は `ParSelect` で確定するまで仮値 4。
    pub fn new(tee_pos: [f64; 3]) -> Self {
        Self {
            phase: RoundPhase::ParSelect,
            par: 4,
            strokes: 0,
            penalties: 0,
            stroke_log: Vec::new(),
            holed_out: false,
            last_start_pos: tee_pos,
            result: None,
        }
    }

    /// `'3'`/`'4'`/`'5'` の入力を Par として受け付けて [`RoundPhase::Playing`]
    /// に遷移する。範囲外や他フェーズからは no-op。
    pub fn select_par(&mut self, par: u32) {
        if self.phase == RoundPhase::ParSelect && (3..=5).contains(&par) {
            self.par = par;
            self.phase = RoundPhase::Playing;
        }
    }

    /// ショット確定時（`Flight` 遷移直前）に呼ぶ。発射位置を記録する。
    pub fn start_stroke(&mut self, start_pos: [f64; 3]) {
        if self.phase == RoundPhase::Playing {
            self.last_start_pos = start_pos;
        }
    }

    /// ショット結果を履歴に追加する。`penalty=true` なら `penalties` も +1。
    /// ホールインしたなら end_pos 記録後に [`RoundState::finish`] を別途呼ぶ。
    pub fn record_stroke(
        &mut self,
        club: Club,
        power: f64,
        start_pos: [f64; 3],
        end_pos: [f64; 3],
        rest_tile: Option<TileType>,
        penalty: bool,
    ) {
        if self.phase != RoundPhase::Playing {
            return;
        }
        self.strokes += 1;
        if penalty {
            self.penalties += 1;
        }
        self.stroke_log.push(StrokeRecord {
            index: self.strokes,
            club,
            power,
            start_pos,
            end_pos,
            rest_tile,
            penalty,
        });
    }

    /// ホールアウトしたときに呼ぶ。`result` を埋めて [`RoundPhase::Finished`]
    /// に遷移する。
    pub fn finish(&mut self) {
        if self.phase != RoundPhase::Playing {
            return;
        }
        self.holed_out = true;
        let total = self.strokes + self.penalties;
        let vs_par = total as i32 - self.par as i32;
        let label = score_label(self.strokes, self.par);
        self.result = Some(RoundResult {
            par: self.par,
            strokes: self.strokes,
            penalties: self.penalties,
            total,
            vs_par,
            label,
        });
        self.phase = RoundPhase::Finished;
    }

    /// リトライ: Par を保持したままスコアとログをリセットして `Playing` に
    /// 戻す。ParSelect には戻らない（仕様: Par 再選択なし）。
    pub fn retry(&mut self, tee_pos: [f64; 3]) {
        self.phase = RoundPhase::Playing;
        self.strokes = 0;
        self.penalties = 0;
        self.stroke_log.clear();
        self.holed_out = false;
        self.last_start_pos = tee_pos;
        self.result = None;
    }
}

/// ホールイン判定。水平距離がカップ半径以下 **かつ** 速度が
/// [`HOLE_OUT_SPEED_LIMIT`] 未満のとき `true`。境界は inclusive（距離
/// そのもので比較するので `dx²+dy²` と `r²` を二乗演算した浮動小数の丸め
/// で境界テストが落ちないように `<= r + ε` に統一する）。
pub fn check_hole_out(ball: &BallState, pin: [f64; 3]) -> bool {
    let dx = ball.pos[0] - pin[0];
    let dy = ball.pos[1] - pin[1];
    let horiz = (dx * dx + dy * dy).sqrt();
    if horiz > CUP_RADIUS_M + 1e-9 {
        return false;
    }
    let s2 = ball.vel[0].powi(2) + ball.vel[1].powi(2) + ball.vel[2].powi(2);
    s2 < HOLE_OUT_SPEED_LIMIT * HOLE_OUT_SPEED_LIMIT
}

/// ストローク数と Par からスコアラベルを決める。`strokes == 1` は Par に
/// かかわらず常に "Hole in One"。
pub fn score_label(strokes: u32, par: u32) -> &'static str {
    if strokes == 1 {
        return "Hole in One";
    }
    let diff = strokes as i32 - par as i32;
    match diff {
        d if d <= -3 => "Albatross",
        -2 => "Eagle",
        -1 => "Birdie",
        0 => "Par",
        1 => "Bogey",
        2 => "Double Bogey",
        3 => "Triple Bogey",
        _ => "Over",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ball(pos: [f64; 3], vel: [f64; 3]) -> BallState {
        BallState {
            pos,
            vel,
            at_rest: false,
        }
    }

    const PIN: [f64; 3] = [190.5, 20.5, 0.0];

    #[test]
    fn check_hole_out_within_radius_and_slow() {
        let b = ball([PIN[0] + 0.03, PIN[1], PIN[2]], [0.5, 0.0, 0.0]);
        assert!(check_hole_out(&b, PIN));
    }

    #[test]
    fn check_hole_out_just_outside_radius() {
        let b = ball([PIN[0] + 0.06, PIN[1], PIN[2]], [0.5, 0.0, 0.0]);
        assert!(!check_hole_out(&b, PIN));
    }

    #[test]
    fn check_hole_out_too_fast_skims_over() {
        let b = ball([PIN[0] + 0.02, PIN[1], PIN[2]], [3.0, 0.0, 0.0]);
        assert!(!check_hole_out(&b, PIN));
    }

    #[test]
    fn check_hole_out_at_exact_radius_boundary() {
        let b = ball([PIN[0] + CUP_RADIUS_M, PIN[1], PIN[2]], [0.0, 0.0, 0.0]);
        assert!(check_hole_out(&b, PIN));
    }

    #[test]
    fn score_label_hole_in_one_overrides_par() {
        assert_eq!(score_label(1, 3), "Hole in One");
        assert_eq!(score_label(1, 4), "Hole in One");
        assert_eq!(score_label(1, 5), "Hole in One");
    }

    #[test]
    fn score_label_par() {
        assert_eq!(score_label(3, 3), "Par");
        assert_eq!(score_label(4, 4), "Par");
        assert_eq!(score_label(5, 5), "Par");
    }

    #[test]
    fn score_label_birdie() {
        assert_eq!(score_label(3, 4), "Birdie");
        assert_eq!(score_label(4, 5), "Birdie");
    }

    #[test]
    fn score_label_eagle() {
        assert_eq!(score_label(2, 4), "Eagle");
        assert_eq!(score_label(3, 5), "Eagle");
    }

    #[test]
    fn score_label_albatross() {
        // par 5 の 3 打 → Albatross。par 4 の 1 打は "Hole in One" が優先。
        assert_eq!(score_label(2, 5), "Albatross");
    }

    #[test]
    fn score_label_bogey() {
        assert_eq!(score_label(5, 4), "Bogey");
    }

    #[test]
    fn score_label_double_and_triple_bogey() {
        assert_eq!(score_label(6, 4), "Double Bogey");
        assert_eq!(score_label(7, 4), "Triple Bogey");
    }

    #[test]
    fn score_label_over() {
        assert_eq!(score_label(9, 4), "Over");
        assert_eq!(score_label(12, 5), "Over");
    }

    #[test]
    fn round_state_new_starts_in_par_select() {
        let r = RoundState::new([5.0, 20.0, 20.0]);
        assert_eq!(r.phase, RoundPhase::ParSelect);
        assert_eq!(r.strokes, 0);
        assert_eq!(r.penalties, 0);
        assert!(!r.holed_out);
        assert!(r.result.is_none());
    }

    #[test]
    fn select_par_transitions_to_playing() {
        let mut r = RoundState::new([5.0, 20.0, 20.0]);
        r.select_par(4);
        assert_eq!(r.phase, RoundPhase::Playing);
        assert_eq!(r.par, 4);
    }

    #[test]
    fn select_par_out_of_range_noop() {
        let mut r = RoundState::new([5.0, 20.0, 20.0]);
        r.select_par(6);
        assert_eq!(r.phase, RoundPhase::ParSelect);
        r.select_par(2);
        assert_eq!(r.phase, RoundPhase::ParSelect);
    }

    #[test]
    fn record_stroke_appends_log_and_increments() {
        let mut r = RoundState::new([5.0, 20.0, 20.0]);
        r.select_par(4);
        r.record_stroke(
            Club::Driver,
            0.9,
            [5.0, 20.0, 20.0],
            [80.0, 20.0, 0.5],
            Some(4),
            false,
        );
        assert_eq!(r.strokes, 1);
        assert_eq!(r.penalties, 0);
        assert_eq!(r.stroke_log.len(), 1);
        assert_eq!(r.stroke_log[0].index, 1);
        assert_eq!(r.stroke_log[0].club, Club::Driver);
    }

    #[test]
    fn record_stroke_with_penalty_increments_penalties() {
        let mut r = RoundState::new([5.0, 20.0, 20.0]);
        r.select_par(4);
        r.record_stroke(
            Club::Iron7,
            0.8,
            [5.0, 20.0, 20.0],
            [107.0, 19.0, -0.2],
            Some(8),
            true,
        );
        assert_eq!(r.strokes, 1);
        assert_eq!(r.penalties, 1);
    }

    #[test]
    fn finish_computes_result_with_vs_par() {
        let mut r = RoundState::new([5.0, 20.0, 20.0]);
        r.select_par(4);
        r.record_stroke(
            Club::Driver,
            0.9,
            [5.0, 20.0, 20.0],
            [90.0, 20.0, 0.5],
            None,
            false,
        );
        r.record_stroke(
            Club::Iron7,
            0.8,
            [90.0, 20.0, 0.5],
            [170.0, 20.0, 0.3],
            None,
            false,
        );
        r.record_stroke(
            Club::Putter,
            0.3,
            [170.0, 20.0, 0.3],
            [190.5, 20.5, 0.0],
            None,
            false,
        );
        r.finish();
        assert_eq!(r.phase, RoundPhase::Finished);
        let res = r.result.as_ref().unwrap();
        assert_eq!(res.par, 4);
        assert_eq!(res.strokes, 3);
        assert_eq!(res.penalties, 0);
        assert_eq!(res.total, 3);
        assert_eq!(res.vs_par, -1);
        assert_eq!(res.label, "Birdie");
        assert!(r.holed_out);
    }

    #[test]
    fn retry_resets_strokes_keeps_par() {
        let mut r = RoundState::new([5.0, 20.0, 20.0]);
        r.select_par(5);
        r.record_stroke(
            Club::Driver,
            0.9,
            [5.0, 20.0, 20.0],
            [90.0, 20.0, 0.5],
            None,
            false,
        );
        r.finish(); // strokes=1 → "Hole in One"
        assert_eq!(r.phase, RoundPhase::Finished);

        r.retry([5.0, 20.0, 20.0]);
        assert_eq!(r.phase, RoundPhase::Playing);
        assert_eq!(r.par, 5);
        assert_eq!(r.strokes, 0);
        assert_eq!(r.penalties, 0);
        assert!(r.stroke_log.is_empty());
        assert!(!r.holed_out);
        assert!(r.result.is_none());
    }

    #[test]
    fn start_stroke_updates_last_start_pos() {
        let mut r = RoundState::new([5.0, 20.0, 20.0]);
        r.select_par(4);
        r.start_stroke([50.0, 20.0, 0.4]);
        assert_eq!(r.last_start_pos, [50.0, 20.0, 0.4]);
    }

    #[test]
    fn record_stroke_ignored_outside_playing() {
        let mut r = RoundState::new([5.0, 20.0, 20.0]);
        // まだ ParSelect。
        r.record_stroke(
            Club::Driver,
            0.5,
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            None,
            false,
        );
        assert_eq!(r.strokes, 0);
    }
}
