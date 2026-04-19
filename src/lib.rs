//! street-golf — TUI street golf game powered by termray and rapier3d.
//!
//! Phase 1 ではコース定義（[`course`]）を導入する。ティー・フェアウェイ・
//! バンカー・ウォーターハザード・グリーン・ピンをレイアウトした 200m × 40m の
//! 合成コースを [`termray::TileMap`] / [`termray::HeightMap`] として公開し、
//! `cargo run --example fly_through` で歩き回れる。以降のフェーズでは
//! ボール物理（`physics`）、ショット入力（`shot`）、HUD（`hud`）を追加予定。

pub mod course;

pub use course::{Course, TILE_BUNKER, TILE_FAIRWAY, TILE_GREEN, TILE_ROUGH, TILE_TEE, TILE_WATER};
