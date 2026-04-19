//! street-golf — TUI street golf game powered by termray and rapier3d.
//!
//! Phase 1 ではコース定義（[`course`]）を導入する。ティー・フェアウェイ・
//! バンカー・ウォーターハザード・グリーン・ピンをレイアウトした 200m × 40m の
//! 合成コースを [`termray::TileMap`] / [`termray::HeightMap`] として公開し、
//! `cargo run --example fly_through` で歩き回れる。Phase 2 でボール物理
//! ([`physics`])、Phase 3 でショット入力 ([`shot`])、Phase 4 でラウンド進行
//! ([`round`]) を追加。

pub mod camera_follow;
pub mod collide;
pub mod course;
pub mod physics;
pub mod round;
pub mod shot;

pub use camera_follow::{FollowCam, FollowMode};
pub use collide::{step_x, step_y};
pub use course::{Course, TILE_BUNKER, TILE_FAIRWAY, TILE_GREEN, TILE_ROUGH, TILE_TEE, TILE_WATER};
pub use physics::{BallState, Physics};
pub use round::{
    CUP_RADIUS_M, HOLE_OUT_SPEED_LIMIT, RoundPhase, RoundResult, RoundState, StrokeRecord,
    check_hole_out, score_label,
};
pub use shot::{Club, ClubSpec, ShotPhase, ShotState};
pub use termray::{TILE_EMPTY, TILE_VOID, TILE_WALL};
