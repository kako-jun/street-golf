# street-golf

## Overview

TUI street golf game. Rust + crossterm. termray-powered raycasting for the view, rapier3d for ball physics. The first shot is a rooftop tee-off; the ball lands on the street and play continues from wherever it rests until you hole out. Real-world data (OSM / SRTM) is out of scope for early phases — Phase 1 uses a synthetic course. OSM/SRTM import lands in Phase 5+.

## Architecture

Single-crate binary depending on the external [`termray`](https://github.com/kako-jun/termray) crate (0.3+) and [`rapier3d`](https://crates.io/crates/rapier3d) (0.32+).

- `src/main.rs` — entry point, input loop, frame presentation (crossterm half-block output)
- `src/lib.rs` — public surface. Empty at Phase 0, will grow with each phase
- `src/course.rs` (Phase 1 — 実装済) — `Course`: implements `termray::TileMap` + `termray::HeightMap`. Hand-drawn 200×40 layout with tee / fairway / rough / bunkers / water / green + pin. Per-corner floor heights are precomputed with a seed-derived low-frequency noise base, a flat 20m rooftop tee, a water surface at -0.2m (solid), and a green bowl centered on the pin.
- `src/physics.rs` (Phase 2) — rapier3d rigid-body world, ball state, course collider generation, step-and-sample integration
- `src/shot.rs` (Phase 3) — shot input (club selection, aim, power) and the shot → ball-flight → rest sequence
- `src/hud.rs` (Phase 4) — score card, club picker, wind / distance / lie indicators

termray (external) owns raycasting: DDA walls, per-column ray-floor intersection, sprite and label projection. street-golf injects golf semantics (course surface heights, ball sprite, hole pin label) into termray through its trait-based hooks, while rapier3d owns the ball's trajectory and the camera rides along.

## Build & run

```bash
cargo run --release
cargo clippy --all-targets -- -D warnings
cargo fmt --all
```

## Current phase

Phase 1 — synthetic course. `cargo run --release --example fly_through` opens a 200m × 40m hand-drawn course (tee / fairway / rough / two bunkers / water hazard / green + pin) on top of termray 0.3's sloped-floor rendering. `Course::generate(42)` is seed-deterministic; the walkthrough lets you verify the terrain before ball physics land in Phase 2. `src/main.rs` still runs the Phase 0 splash — it stays untouched until Phase 2 wires rapier3d in.

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
