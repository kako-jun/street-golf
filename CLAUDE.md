# street-golf

## Overview

TUI street golf game. Rust + crossterm. termray-powered raycasting for the view, rapier3d for ball physics. The first shot is a rooftop tee-off; the ball lands on the street and play continues from wherever it rests until you hole out. Real-world data (OSM / SRTM) is out of scope for early phases — Phase 1 uses a synthetic course. OSM/SRTM import lands in Phase 5+.

## Architecture

Single-crate binary depending on the external [`termray`](https://github.com/kako-jun/termray) crate (0.3+) and [`rapier3d`](https://crates.io/crates/rapier3d) (0.32+).

- `src/main.rs` — entry point, input loop, frame presentation (crossterm half-block output)
- `src/lib.rs` — public surface. Empty at Phase 0, will grow with each phase
- `src/course.rs` (Phase 1 — 実装済) — `Course`: implements `termray::TileMap` + `termray::HeightMap`. Hand-drawn 200×40 layout with tee / fairway / rough / bunkers / water / green + pin. Per-corner floor heights are precomputed with a seed-derived low-frequency noise base, a flat 20m rooftop tee, a water surface at -0.2m (solid), and a green bowl centered on the pin.
- `src/physics.rs` (Phase 2) — rapier3d rigid-body world, ball state, course collider generation, step-and-sample integration
- `src/shot.rs` (Phase 3 — 実装済) — `ShotState` ステートマシン (`Aiming` / `PowerSwinging` / `Flight` / `AtRest`)、`Club` / `ClubSpec` テーブル、自己振動するパワーメーター、launch 速度計算。純粋ロジックでユニットテストのみ
- `src/round.rs` (Phase 4 — 実装済) — `RoundState` ステートマシン (`ParSelect` / `Playing` / `Finished`)、`check_hole_out` 判定（カップ半径 + 速度上限）、`score_label`、ストローク履歴 `StrokeRecord`、集計 `RoundResult`。Par 選択 / 水ペナルティ / リトライを含む。純粋ロジック

termray (external) owns raycasting: DDA walls, per-column ray-floor intersection, sprite and label projection. street-golf injects golf semantics (course surface heights, ball sprite, hole pin label) into termray through its trait-based hooks, while rapier3d owns the ball's trajectory and the camera rides along.

## Build & run

```bash
cargo run --release
cargo clippy --all-targets -- -D warnings
cargo fmt --all
```

## Current phase

Phase 4 — ホールアウト判定 + スコアカード + ラウンド終了。MVP v0.1.0 としての遊び切れる 1 ホールが完成した。`src/round.rs` に `RoundState` を追加し、`ParSelect → Playing → Finished` の 3 ステートで進行管理する。起動直後に Par 3 / 4 / 5 を選び、`Playing` フェーズで Phase 3 のショットループが回る。ボールが静止するたびにカップ判定（半径 5.4cm 以内 **かつ** 速度 2m/s 未満）と水ペナルティ判定（`TILE_WATER` で静止 → +1 打 + 発射位置に復帰）を挟み、ホールインすれば `Finished` に遷移して最終スコア（Hole in One / Albatross / Eagle / Birdie / Par / Bogey / …）とストローク履歴を表示する。`Y` でリトライ（Par は維持）、`N` / `Esc` で終了。`examples/shot_test.rs` は Phase 2 の物理検証用ハーネスとして据え置き。

### 操作キー

| キー | フェーズ | 効果 |
|---|---|---|
| `3` / `4` / `5` | ParSelect | Par 3 / 4 / 5 を選んで Playing に遷移 |
| `1`-`8` | Playing | クラブ選択（Driver / 3I / 5I / 7I / 9I / PW / SW / Putter） |
| `a` / `d` | Playing | yaw（水平方向）を ±約 2° |
| `w` / `s` | Playing | pitch（打ち出し角）を ±2°（0°-45° クランプ） |
| `Space` | Playing | Aiming→PowerSwinging→Flight、AtRest→Aiming |
| `x` | Playing | PowerSwinging→Aiming（スイングをキャンセル） |
| `Y` | Finished | 同じ Par でリトライ |
| `N` | Finished | 終了 |
| `Esc` | 全フェーズ | 終了 |

## Roadmap

| Phase | Issue | Scope |
|---|---|---|
| 0 | #1 | Project scaffolding (this) |
| 1 | #2 | Synthetic terrain + 1 hole |
| 2 | #3 | rapier3d ball physics ↔ termray camera |
| 3 | #4 | Shot input + ball sequence |
| 4 | #5 | Hole-out + score + round end |
| 5+ | — | OSM/SRTM real-world data import |

## Repo rules

- Do not add `Co-Authored-By` to commits. No AI footprints in git history.
- Do not use `--no-verify`.
- Commit messages are Conventional Commits in Japanese with the issue number, e.g. `feat: kako-jun/street-golf#1 Phase 0 プロジェクト骨格を追加`.
- No emoji in code, commits, or docs.
- Keep comments sparse — match the density of `src/main.rs` in Phase 0.
