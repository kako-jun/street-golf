# street-golf

TUI street golf — tee off from a rooftop, land on the street, keep playing until you hole out.

> **Status**: Pre-alpha. Phase 2 wires rapier3d ball physics into the termray view. Shot input and scoring still land in later phases.

## Stack

- Rust 2024 (MSRV 1.85)
- [termray](https://crates.io/crates/termray) 0.3 — the TUI raycasting engine that powers rendering
- [rapier3d](https://crates.io/crates/rapier3d) 0.32 — 3D rigid-body physics for the ball
- [crossterm](https://crates.io/crates/crossterm) — input and terminal I/O

## Build

```bash
cargo build --release
./target/release/street-golf
cargo run --release --example fly_through   # Phase 1 walk-through of the synthetic course
cargo run --release --example shot_test     # Phase 2 hardcoded 1-shot physics demo
```

The Phase 0 binary still clears the terminal to a dark meadow green for about 0.8 seconds and exits. The Phase 1 `fly_through` example lets you stroll the 200×40 synthetic course. The Phase 2 `shot_test` example fires a preset shot at 0.5s and lets you watch the ball fly, land, roll, and come to rest under rapier3d, with a third-person ShotStanding follow camera.

## Roadmap

| Phase | Issue | Scope |
|---|---|---|
| 0 | [#1](https://github.com/kako-jun/street-golf/issues/1) | Project scaffolding (this) |
| 1 | #2 | Synthetic terrain + 1 hole |
| 2 | #3 | rapier3d ball physics ↔ termray camera |
| 3 | #4 | Shot input + ball sequence |
| 4 | #5 | Hole-out + score + round end |
| 5+ | — | OSM/SRTM real-world data import |

## License

MIT. See [LICENSE](LICENSE).
