# Changelog

All notable changes to street-golf are documented in this file. The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres to
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed
- 描画前に framebuffer を「水色の空 + 緑の地面」で塗りつぶすようになった
  (`fill_sky_ground` in `src/main.rs`)。`render_floor_ceiling` は per-column
  DDA を `MAX_DISTANCE = 180m` までしか歩かないため地平線付近に塗り残しが
  発生し、`fb.clear(Color::default())` のままだとそこが黒くてさみしい見た目
  になっていた。`Camera` の `pitch` を horizon shift として扱う擬似ピッチに
  追従して水平線を計算するので、上を向いても下を向いても 2 トーンが正しく
  ずれる。
- ボールスプライトを `.#./###/.#.`（3x3 十字）から 5x5 のコーナーを欠いた
  塊に変更。近距離でプラス記号に見えていたのを丸っぽい塊に。
- 静止判定の閾値を `|linvel| < 0.05 ∧ |angvel| < 0.1` 0.5s 保持から
  `|linvel| < 0.25 ∧ |angvel| < 0.5` 0.25s 保持に緩和
  (`src/physics.rs`)。実球は 0.05 m/s でも転がり続けるため、プレイヤー
  体感の「もう動いてない」に合わせた。`FLIGHT_REST_HOLD_SEC` のドキュ
  コメントも整合する値に更新。

### Fixed
- コース外（マップ外 or 地表 z=20 より十分下）に出たボールが永遠に落下し
  ラウンドが進まなくなる不具合を修正。壁コライダが z=0..10 にしかなく強い
  ショットで壁の上を抜けると地形コライダが無いため、`tile_at` が `None` か
  `ball.pos.z < 10.0` になった時点で `at_rest` を待たず水ペナルティと同じ
  復帰処理を走らせる (`src/main.rs`)。壁コライダ自体を高くする根本修正は
  別 Issue 扱い。

### Added
- Phase 4 hole-out + score card + round end
  ([#5](https://github.com/kako-jun/street-golf/issues/5)).
  Phase 4 の完了で **MVP v0.1.0** としての 1 ホール分のプレイアブル体験が
  揃った（crates.io 公開は別 Issue で扱う）。`src/round.rs` に
  `RoundState` を追加し、`ParSelect → Playing → Finished` の 3 ステートで
  進行管理する。`check_hole_out` はカップ半径 5.4cm（R&A/USGA 規格の
  10.8cm 直径）と速度上限 2m/s の AND 条件で skim-over を防ぎ、
  `score_label` は Hole in One / Albatross / Eagle / Birdie / Par / Bogey /
  Double Bogey / Triple Bogey / Over を返す。`Physics::teleport_ball`
  と `Course::pin_world_pos` を追加し、水タイルで静止すれば 1 打罰して
  発射位置に戻す。`main.rs` は Par 選択画面（Par 3 / 4 / 5）、プレイ中の
  スコア HUD、ホールアウト後の結果ダイアログ（framebuffer 中央を薄暗く
  してオーバーレイ + ストローク履歴）、`Y` でのリトライ（Par は維持）を
  実装。21 件のユニットテスト（round）+ 2 件（physics / course）を追加、
  合計 69 件が通る。

- Phase 3 shot input + swing sequence
  ([#4](https://github.com/kako-jun/street-golf/issues/4)).
  `src/shot.rs` introduces the pure-logic `ShotState` machine with four phases
  (`Aiming` → `PowerSwinging` → `Flight` → `AtRest`), the 8-club `Club` /
  `ClubSpec` table (Driver / 3I / 5I / 7I / 9I / PW / SW / Putter), and a
  self-oscillating triangle-wave power meter (Space to start and stop, since
  crossterm's `KeyRelease` is not portable). 16 unit tests cover the
  oscillator, velocity formula, state transitions, club-switch /
  loft-override behaviour, yaw wrap-around, and swing cancel (`x`).

- Phase 2 rapier3d ball physics + follow camera
  ([#3](https://github.com/kako-jun/street-golf/issues/3)).
  `src/physics.rs` wraps a rapier3d 0.32 world with per-tile-type TriMesh
  colliders generated from `Course::cell_heights` using the
  `(NW,NE,SE)+(NW,SE,SW)` triangulation that matches termray's floor renderer,
  plus vertical cuboid colliders on wall cells. The ball is a 46 g dynamic body
  of radius 2.1 cm with CCD. `Physics::step` runs fixed 60Hz steps via an
  accumulator so render rate stays independent. `src/camera_follow.rs` adds
  `FollowCam` with the third-person `ShotStanding` mode (Flying / FirstPerson
  fall through until Phase 3). `examples/shot_test.rs` fires a hardcoded
  `(12, 0, 4) m/s` launch at 0.5s and visualises the flight-to-rest sequence.
- Phase 1 synthetic course ([#2](https://github.com/kako-jun/street-golf/issues/2)).
  `src/course.rs` introduces `Course`, which implements both `termray::TileMap`
  and `termray::HeightMap` for a hand-drawn 200m × 40m layout (rooftop tee,
  fairway, rough, two bunkers, water hazard, green with pin). `Course::generate(seed)`
  is seed-deterministic; corner heights are precomputed so the `HeightMap`
  continuity contract is automatically satisfied. `examples/fly_through.rs` lets
  you walk the course (`cargo run --release --example fly_through`) to verify
  the terrain before ball physics lands in Phase 2.
- Project skeleton on top of termray 0.3 and rapier3d 0.32
  ([#1](https://github.com/kako-jun/street-golf/issues/1)). `cargo run` blanks the
  framebuffer to a meadow green for a brief moment and exits cleanly — Phase 1+ will
  turn this into a playable street golf game.
- MIT `LICENSE`, `README.md`, `CLAUDE.md`, and `docs/architecture.md` describing the
  termray + rapier3d design and the Phase 0 → 5+ roadmap.
- GitHub Actions workflows: `ci.yml` (fmt / clippy / test / release build on every push
  to `main` and every PR) and `release.yml` (five target binaries — linux x86_64 /
  aarch64, macOS x86_64 / aarch64, windows x86_64 — on `v*` tags, uploaded via
  `softprops/action-gh-release@v2`).

### Changed
- `src/main.rs` は Phase 3 のショットループを `RoundState` でラップした
  3 フェーズ制に拡張。Par 選択 → ショットループ → 結果画面 → リトライの
  全フローが動く。HUD 行数を 3 → 8 に拡張（結果画面の履歴表示用）。
- `src/main.rs` replaces the Phase 0 800ms splash with the full Phase 3 game
  loop: input → state tick → physics step → camera update → render → HUD.
  `cargo run --release` is now the playable binary.

### Notes
- Phase 4 完了で MVP v0.1.0 相当の遊びが揃ったが、crates.io への公開は
  別 Issue（Phase 5+ でまとめて扱う）。タグは `[Unreleased]` のまま据え置き。
- `examples/shot_test.rs` は Phase 2 のハードコード発射検証として据え置き。`ShotState` とは独立。
