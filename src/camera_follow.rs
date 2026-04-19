//! 3 人称追従カメラ。
//!
//! [`FollowMode::ShotStanding`] はボールの後方 5m・上空 2m から見下ろす
//! カメラ姿勢を返す。Flying / FirstPerson は Phase 3 以降で実装予定で、
//! 現状は ShotStanding と同じ計算にフォールバックする。
//!
//! 返り値は termray の [`termray::Camera`] に渡せる `(x, y, z, yaw, pitch)`。

/// 追従モード。現状 MVP では ShotStanding のみ実装。
pub enum FollowMode {
    ShotStanding,
    Flying,
    FirstPerson,
}

/// 追従カメラの状態。`yaw` はショットの狙い方向。
pub struct FollowCam {
    pub mode: FollowMode,
    pub yaw: f64,
}

const STANDING_BACK: f64 = 5.0;
const STANDING_UP: f64 = 2.0;
const STANDING_PITCH: f64 = -0.15;

impl FollowCam {
    pub fn new(mode: FollowMode, yaw: f64) -> Self {
        Self { mode, yaw }
    }

    /// ボールのワールド座標から `(cam_x, cam_y, cam_z, cam_yaw, cam_pitch)` を返す。
    pub fn update(&self, ball: [f64; 3]) -> (f64, f64, f64, f64, f64) {
        match self.mode {
            FollowMode::ShotStanding | FollowMode::Flying | FollowMode::FirstPerson => {
                self.shot_standing(ball)
            }
        }
    }

    fn shot_standing(&self, ball: [f64; 3]) -> (f64, f64, f64, f64, f64) {
        let fx = self.yaw.cos();
        let fy = self.yaw.sin();
        let cx = ball[0] - fx * STANDING_BACK;
        let cy = ball[1] - fy * STANDING_BACK;
        let cz = ball[2] + STANDING_UP;
        (cx, cy, cz, self.yaw, STANDING_PITCH)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shot_standing_places_camera_behind_ball() {
        let cam = FollowCam::new(FollowMode::ShotStanding, 0.0);
        let (cx, cy, cz, cyaw, cpitch) = cam.update([10.0, 5.0, 1.0]);
        // yaw=0 は +X 方向を見る。カメラは ball から -X に 5m。
        assert!((cx - 5.0).abs() < 1e-9);
        assert!((cy - 5.0).abs() < 1e-9);
        assert!((cz - 3.0).abs() < 1e-9);
        assert!((cyaw - 0.0).abs() < 1e-9);
        assert!((cpitch - (-0.15)).abs() < 1e-9);
    }

    #[test]
    fn shot_standing_respects_yaw() {
        let cam = FollowCam::new(FollowMode::ShotStanding, std::f64::consts::FRAC_PI_2);
        let (cx, cy, _cz, _cyaw, _cpitch) = cam.update([10.0, 10.0, 1.0]);
        // yaw=π/2 は +Y 方向を見る。カメラは ball から -Y に 5m。
        assert!((cx - 10.0).abs() < 1e-6);
        assert!((cy - 5.0).abs() < 1e-6);
    }

    #[test]
    fn flying_and_first_person_fall_through_to_standing() {
        let a = FollowCam::new(FollowMode::Flying, 0.0).update([0.0, 0.0, 0.0]);
        let b = FollowCam::new(FollowMode::FirstPerson, 0.0).update([0.0, 0.0, 0.0]);
        let c = FollowCam::new(FollowMode::ShotStanding, 0.0).update([0.0, 0.0, 0.0]);
        assert_eq!(a, b);
        assert_eq!(b, c);
    }
}
