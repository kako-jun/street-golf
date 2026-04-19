//! Phase 3 のショット入力ステートマシン。
//!
//! 狙い (yaw / pitch)・クラブ選択・パワーメーターの自己振動を一元管理する
//! 純粋ロジック。crossterm や termray とは独立しているのでユニットテストで
//! 完全にカバーできる。`src/main.rs` が入力イベントを [`ShotState`] に流し
//! 込み、[`ShotPhase::Flight`] に遷移したタイミングで `Physics::launch`
//! を呼ぶ。
//!
//! パワーメーターは「押しっぱなしで貯める」方式ではなく、`PowerSwinging`
//! 中に自動で 0→1→0 の三角波として揺れる。crossterm の KeyRelease は
//! プラットフォーム依存で取れないターミナルが多いため、Space 2 回押し
//! （開始 / 停止）の方がポータブルである。

/// パワーメーターが 0→1→0 を一周するのに要する秒数。
pub const POWER_CYCLE_SEC: f64 = 2.4;
/// パワーメーター半周 = フル充填までの秒数。
pub const POWER_HALF_SEC: f64 = POWER_CYCLE_SEC * 0.5;
/// Aiming 中に `w/s` で調整できる打ち出し角の上限（度）。
pub const MAX_PITCH_DEG: f64 = 45.0;
/// `a/d` 1 押しあたりの yaw 変化量（rad、約 2°）。
pub const YAW_STEP_RAD: f64 = 0.035;
/// `w/s` 1 押しあたりの pitch 変化量（度）。
pub const PITCH_STEP_DEG: f64 = 2.0;

/// クラブ種別。`Driver` が最大飛距離、`Putter` が最小。
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Club {
    Driver,
    Iron3,
    Iron5,
    Iron7,
    Iron9,
    PitchingWedge,
    SandWedge,
    Putter,
}

/// クラブの物理パラメータ。`loft_deg` はデフォルトの打ち出し角。
pub struct ClubSpec {
    pub max_speed_mps: f64,
    pub loft_deg: f64,
    pub label: &'static str,
}

impl Club {
    /// クラブ定数テーブル。値は MVP 用の素朴な近似で、物理的な実測ではない。
    pub fn spec(self) -> ClubSpec {
        match self {
            Club::Driver => ClubSpec {
                max_speed_mps: 65.0,
                loft_deg: 11.0,
                label: "Driver",
            },
            Club::Iron3 => ClubSpec {
                max_speed_mps: 60.0,
                loft_deg: 21.0,
                label: "3I",
            },
            Club::Iron5 => ClubSpec {
                max_speed_mps: 55.0,
                loft_deg: 26.0,
                label: "5I",
            },
            Club::Iron7 => ClubSpec {
                max_speed_mps: 50.0,
                loft_deg: 33.0,
                label: "7I",
            },
            Club::Iron9 => ClubSpec {
                max_speed_mps: 43.0,
                loft_deg: 40.0,
                label: "9I",
            },
            Club::PitchingWedge => ClubSpec {
                max_speed_mps: 36.0,
                loft_deg: 45.0,
                label: "PW",
            },
            Club::SandWedge => ClubSpec {
                max_speed_mps: 28.0,
                loft_deg: 54.0,
                label: "SW",
            },
            Club::Putter => ClubSpec {
                max_speed_mps: 18.0,
                loft_deg: 0.0,
                label: "Putter",
            },
        }
    }

    /// `'1'..='8'` の入力をクラブにマップする。`'9'` は将来の Lob 用に予約。
    pub fn from_digit(c: char) -> Option<Self> {
        match c {
            '1' => Some(Club::Driver),
            '2' => Some(Club::Iron3),
            '3' => Some(Club::Iron5),
            '4' => Some(Club::Iron7),
            '5' => Some(Club::Iron9),
            '6' => Some(Club::PitchingWedge),
            '7' => Some(Club::SandWedge),
            '8' => Some(Club::Putter),
            _ => None,
        }
    }
}

/// ショットの進行段階。
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ShotPhase {
    /// 狙いを調整中。`a/d/w/s/1-8` を受け付ける。
    Aiming,
    /// パワーメーターが振動中。Space 2 回目で [`ShotPhase::Flight`] に遷移。
    PowerSwinging,
    /// ボールが飛翔中 / 転動中。入力は受け付けない。
    Flight,
    /// ボール静止。Space で [`ShotPhase::Aiming`] に戻る。
    AtRest,
}

/// ショット入力の全状態。`Physics` とは独立に保持する。
pub struct ShotState {
    pub phase: ShotPhase,
    pub club: Club,
    pub yaw: f64,
    pub pitch_deg: f64,
    /// 現在のパワー値 (`0.0..=1.0`)。PowerSwinging 中は三角波として更新される。
    pub power: f64,
    /// 三角波の位相 (`0.0..POWER_CYCLE_SEC`)。
    pub power_t: f64,
    pub shots: u32,
    /// `w/s` で pitch を手動調整した場合のみ `true`。
    /// `true` のとき、クラブ切替時に loft から pitch を上書きしない。
    pub loft_override: bool,
}

impl ShotState {
    pub fn new(initial_yaw: f64) -> Self {
        Self {
            phase: ShotPhase::Aiming,
            club: Club::Driver,
            yaw: initial_yaw,
            pitch_deg: Club::Driver.spec().loft_deg,
            power: 0.0,
            power_t: 0.0,
            shots: 0,
            loft_override: false,
        }
    }

    /// `PowerSwinging` 中のみ `power_t` / `power` を `dt` 秒分進める。
    /// `power` は `0→1→0` の三角波。
    pub fn tick(&mut self, dt: f64) {
        if self.phase != ShotPhase::PowerSwinging {
            return;
        }
        self.power_t = (self.power_t + dt).rem_euclid(POWER_CYCLE_SEC);
        let t = self.power_t / POWER_HALF_SEC;
        self.power = if t <= 1.0 { t } else { 2.0 - t };
    }

    /// Space キー押下。Phase を次に進める。戻り値の `true` はちょうど
    /// `Flight` に遷移した直後（= `Physics::launch` を呼ぶべきタイミング）。
    pub fn press_space(&mut self) -> bool {
        match self.phase {
            ShotPhase::Aiming => {
                self.phase = ShotPhase::PowerSwinging;
                self.power_t = 0.0;
                self.power = 0.0;
                false
            }
            ShotPhase::PowerSwinging => {
                self.phase = ShotPhase::Flight;
                self.shots += 1;
                true
            }
            ShotPhase::AtRest => {
                self.phase = ShotPhase::Aiming;
                self.power = 0.0;
                self.power_t = 0.0;
                false
            }
            ShotPhase::Flight => false,
        }
    }

    /// 物理側でボールが静止したら呼ぶ。`Flight` からのみ `AtRest` に遷移する。
    pub fn notify_rest(&mut self) {
        if self.phase == ShotPhase::Flight {
            self.phase = ShotPhase::AtRest;
        }
    }

    /// 現在の狙い・クラブ・パワーから launch 速度ベクトルを計算する。
    /// `v = speed * (cos(pitch)cos(yaw), cos(pitch)sin(yaw), sin(pitch))`。
    pub fn compute_launch_velocity(&self) -> [f64; 3] {
        let speed = self.club.spec().max_speed_mps * self.power;
        let pitch = self.effective_pitch_deg().to_radians();
        let c = pitch.cos();
        let vx = speed * c * self.yaw.cos();
        let vy = speed * c * self.yaw.sin();
        let vz = speed * pitch.sin();
        [vx, vy, vz]
    }

    /// 実効的な打ち出し角。`loft_override` ならユーザー調整の `pitch_deg`、
    /// そうでなければクラブの loft。
    pub fn effective_pitch_deg(&self) -> f64 {
        if self.loft_override {
            self.pitch_deg
        } else {
            self.club.spec().loft_deg
        }
    }

    pub fn adjust_yaw(&mut self, delta_rad: f64) {
        if self.phase == ShotPhase::Aiming {
            self.yaw += delta_rad;
        }
    }

    pub fn adjust_pitch(&mut self, delta_deg: f64) {
        if self.phase == ShotPhase::Aiming {
            self.pitch_deg = (self.pitch_deg + delta_deg).clamp(0.0, MAX_PITCH_DEG);
            self.loft_override = true;
        }
    }

    pub fn select_club(&mut self, club: Club) {
        if self.phase == ShotPhase::Aiming {
            self.club = club;
            if !self.loft_override {
                self.pitch_deg = club.spec().loft_deg;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oscillator_returns_to_zero_after_full_cycle() {
        let mut s = ShotState::new(0.0);
        s.press_space();
        s.tick(POWER_CYCLE_SEC);
        assert!(
            s.power_t.abs() < 1e-9,
            "power_t should wrap to 0, got {}",
            s.power_t
        );
        assert!(s.power.abs() < 1e-9, "power should be 0, got {}", s.power);
    }

    #[test]
    fn power_is_triangle_wave() {
        let mut s = ShotState::new(0.0);
        s.press_space();
        s.tick(POWER_HALF_SEC);
        assert!(
            (s.power - 1.0).abs() < 1e-9,
            "power at half cycle should be 1.0, got {}",
            s.power
        );
        s.tick(POWER_HALF_SEC);
        assert!(
            s.power.abs() < 1e-9,
            "power at full cycle should be 0, got {}",
            s.power
        );
    }

    #[test]
    fn compute_velocity_zero_pitch() {
        let mut s = ShotState::new(0.0);
        s.adjust_pitch(-Club::Driver.spec().loft_deg); // pitch=0 にクランプ
        s.press_space();
        s.tick(POWER_HALF_SEC); // power=1.0
        let v = s.compute_launch_velocity();
        assert!((v[0] - 65.0).abs() < 1e-6, "vx ≈ 65, got {}", v[0]);
        assert!(v[1].abs() < 1e-6, "vy ≈ 0, got {}", v[1]);
        assert!(v[2].abs() < 1e-6, "vz ≈ 0, got {}", v[2]);
    }

    #[test]
    fn compute_velocity_45_pitch() {
        let mut s = ShotState::new(0.0);
        // Driver loft 11° から +34° 調整して 45°
        s.adjust_pitch(45.0 - Club::Driver.spec().loft_deg);
        s.press_space();
        s.tick(POWER_HALF_SEC);
        let v = s.compute_launch_velocity();
        let expected = 65.0 / 2.0_f64.sqrt();
        assert!((v[0] - expected).abs() < 1e-6, "vx ≈ 65/√2, got {}", v[0]);
        assert!(v[1].abs() < 1e-6, "vy ≈ 0, got {}", v[1]);
        assert!((v[2] - expected).abs() < 1e-6, "vz ≈ 65/√2, got {}", v[2]);
    }

    #[test]
    fn yaw_changes_horizontal_direction() {
        let mut s = ShotState::new(std::f64::consts::FRAC_PI_2);
        s.adjust_pitch(-Club::Driver.spec().loft_deg);
        s.press_space();
        s.tick(POWER_HALF_SEC);
        let v = s.compute_launch_velocity();
        assert!(v[0].abs() < 1e-6, "vx ≈ 0, got {}", v[0]);
        assert!((v[1] - 65.0).abs() < 1e-6, "vy ≈ 65, got {}", v[1]);
        assert!(v[2].abs() < 1e-6, "vz ≈ 0, got {}", v[2]);
    }

    #[test]
    fn driver_spec_matches_table() {
        let cases = [
            (Club::Driver, 65.0, 11.0, "Driver"),
            (Club::Iron3, 60.0, 21.0, "3I"),
            (Club::Iron5, 55.0, 26.0, "5I"),
            (Club::Iron7, 50.0, 33.0, "7I"),
            (Club::Iron9, 43.0, 40.0, "9I"),
            (Club::PitchingWedge, 36.0, 45.0, "PW"),
            (Club::SandWedge, 28.0, 54.0, "SW"),
            (Club::Putter, 18.0, 0.0, "Putter"),
        ];
        for (club, speed, loft, label) in cases {
            let spec = club.spec();
            assert_eq!(spec.max_speed_mps, speed, "{:?}", club);
            assert_eq!(spec.loft_deg, loft, "{:?}", club);
            assert_eq!(spec.label, label, "{:?}", club);
        }
    }

    #[test]
    fn putter_has_zero_loft() {
        assert_eq!(Club::Putter.spec().loft_deg, 0.0);
    }

    #[test]
    fn club_from_digit_mapping() {
        assert_eq!(Club::from_digit('1'), Some(Club::Driver));
        assert_eq!(Club::from_digit('2'), Some(Club::Iron3));
        assert_eq!(Club::from_digit('3'), Some(Club::Iron5));
        assert_eq!(Club::from_digit('4'), Some(Club::Iron7));
        assert_eq!(Club::from_digit('5'), Some(Club::Iron9));
        assert_eq!(Club::from_digit('6'), Some(Club::PitchingWedge));
        assert_eq!(Club::from_digit('7'), Some(Club::SandWedge));
        assert_eq!(Club::from_digit('8'), Some(Club::Putter));
        assert_eq!(Club::from_digit('9'), None);
        assert_eq!(Club::from_digit('a'), None);
        assert_eq!(Club::from_digit('0'), None);
    }

    #[test]
    fn press_space_transitions_aim_to_swing_to_flight() {
        let mut s = ShotState::new(0.0);
        assert_eq!(s.phase, ShotPhase::Aiming);
        let launched = s.press_space();
        assert_eq!(s.phase, ShotPhase::PowerSwinging);
        assert!(!launched);
        let launched = s.press_space();
        assert_eq!(s.phase, ShotPhase::Flight);
        assert!(launched);
        assert_eq!(s.shots, 1);
    }

    #[test]
    fn tick_outside_swing_is_noop() {
        let mut s = ShotState::new(0.0);
        let before = s.power_t;
        s.tick(1.0);
        assert_eq!(s.power_t, before);
        assert_eq!(s.power, 0.0);
    }

    #[test]
    fn adjust_yaw_locked_during_flight() {
        let mut s = ShotState::new(0.0);
        s.press_space();
        s.press_space(); // → Flight
        let before = s.yaw;
        s.adjust_yaw(YAW_STEP_RAD);
        assert_eq!(s.yaw, before);
    }

    #[test]
    fn select_club_updates_pitch_unless_override() {
        let mut s = ShotState::new(0.0);
        assert_eq!(s.pitch_deg, Club::Driver.spec().loft_deg);
        s.select_club(Club::Iron7);
        assert_eq!(s.pitch_deg, Club::Iron7.spec().loft_deg);
        // pitch を手動調整するとオーバーライドがかかる。
        s.adjust_pitch(5.0);
        assert!(s.loft_override);
        let manual = s.pitch_deg;
        // 別クラブに切り替えても pitch は変わらない。
        s.select_club(Club::Iron3);
        assert_eq!(s.pitch_deg, manual);
    }

    #[test]
    fn notify_rest_only_from_flight() {
        let mut s = ShotState::new(0.0);
        s.notify_rest();
        assert_eq!(s.phase, ShotPhase::Aiming);
        s.press_space();
        s.notify_rest();
        assert_eq!(s.phase, ShotPhase::PowerSwinging);
        s.press_space(); // → Flight
        s.notify_rest();
        assert_eq!(s.phase, ShotPhase::AtRest);
    }

    #[test]
    fn pitch_clamped_to_range() {
        let mut s = ShotState::new(0.0);
        for _ in 0..100 {
            s.adjust_pitch(PITCH_STEP_DEG);
        }
        assert!(s.pitch_deg <= MAX_PITCH_DEG);
        assert_eq!(s.pitch_deg, MAX_PITCH_DEG);
        for _ in 0..100 {
            s.adjust_pitch(-PITCH_STEP_DEG);
        }
        assert_eq!(s.pitch_deg, 0.0);
    }
}
