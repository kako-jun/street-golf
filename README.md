# street-golf

TUI street golf — tee off from a rooftop, land on the street, keep playing until you hole out.

> **Status**: Pre-alpha. Only the skeleton works right now (Phase 0 — see [#1](https://github.com/kako-jun/street-golf/issues/1)). Expect the repo to grow Phase by Phase into a playable TUI golf game.

## Stack

- Rust 2024 (MSRV 1.85)
- [termray](https://crates.io/crates/termray) 0.3 — the TUI raycasting engine that powers rendering
- [rapier3d](https://crates.io/crates/rapier3d) 0.32 — 3D rigid-body physics for the ball
- [crossterm](https://crates.io/crates/crossterm) — input and terminal I/O

## Build

```bash
cargo build --release
./target/release/street-golf
```

At Phase 0 the binary clears the terminal to a dark meadow green for about 0.8 seconds, then exits — just enough to prove the render pipeline links up. Phase 1 will build a synthetic course and render one playable hole.

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
