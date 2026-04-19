# Changelog

All notable changes to street-golf are documented in this file. The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres to
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Phase 3 shot input + swing sequence
  ([#4](https://github.com/kako-jun/street-golf/issues/4)).
  `src/shot.rs` introduces the pure-logic `ShotState` machine with four phases
  (`Aiming` → `PowerSwinging` → `Flight` → `AtRest`), the 8-club `Club` /
  `ClubSpec` table (Driver / 3I / 5I / 7I / 9I / PW / SW / Putter), and a
  self-oscillating triangle-wave power meter (Space to start and stop, since
  crossterm's `KeyRelease` is not portable). 14 unit tests cover the
  oscillator, velocity formula, state transitions, and club-switch /
  loft-override behaviour.

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
- `src/main.rs` replaces the Phase 0 800ms splash with the full Phase 3 game
  loop: input → state tick → physics step → camera update → render → HUD.
  `cargo run --release` is now the playable binary.
