# street-golf

TUI street golf — tee off from a rooftop, land on the street, keep playing until you hole out.

> **Status**: Pre-alpha. Phase 3 adds the playable shot loop — aim, choose a club, time the power meter, and watch the ball fly until it rests. Scoring and hole-out still land in Phase 4.

## Stack

- Rust 2024 (MSRV 1.85)
- [termray](https://crates.io/crates/termray) 0.3 — the TUI raycasting engine that powers rendering
- [rapier3d](https://crates.io/crates/rapier3d) 0.32 — 3D rigid-body physics for the ball
- [crossterm](https://crates.io/crates/crossterm) — input and terminal I/O

## Build

```bash
cargo build --release
cargo run --release                          # Phase 3 playable shot loop (main binary)
cargo run --release --example fly_through    # Phase 1 walk-through of the synthetic course
cargo run --release --example shot_test      # Phase 2 hardcoded 1-shot physics verification
```

`cargo run --release` now launches the playable game: you tee off from the rooftop, pick a club, aim, and time the power meter to hit the ball toward the pin. The `fly_through` example still walks the 200×40 synthetic course; the `shot_test` example remains a hardcoded launch verification harness for the physics pipeline.

### Controls

| Key | Effect |
|---|---|
| `1`-`8` | Club: Driver / 3I / 5I / 7I / 9I / PW / SW / Putter |
| `a` / `d` | Yaw left / right (~2°) |
| `w` / `s` | Pitch up / down (0°-45°) |
| `Space` | Aim → start swing → release → (after rest) back to aim |
| `Esc` | Quit |

The power meter auto-oscillates from 0% to 100% and back. Press `Space` once to start the swing and a second time to release — whichever power value is displayed at that instant is the shot's strength.

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
