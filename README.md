# street-golf

TUI street golf — tee off from a rooftop, land on the street, keep playing until you hole out.

> **Status**: MVP v0.1.0 — a single playable hole. Phase 4 completes the loop: choose par, play until you hole out, see your score, retry or quit. crates.io publish is tracked separately.

## Stack

- Rust 2024 (MSRV 1.85)
- [termray](https://crates.io/crates/termray) 0.3 — the TUI raycasting engine that powers rendering
- [rapier3d](https://crates.io/crates/rapier3d) 0.32 — 3D rigid-body physics for the ball
- [crossterm](https://crates.io/crates/crossterm) — input and terminal I/O

## Build

```bash
cargo build --release
cargo run --release                          # Phase 4 playable single hole (main binary)
cargo run --release --example fly_through    # Phase 1 walk-through of the synthetic course
cargo run --release --example shot_test      # Phase 2 hardcoded 1-shot physics verification
```

`cargo run --release` launches the playable game: pick a par, tee off from the rooftop, aim, and time the power meter to hit the ball toward the pin. Land it in the cup (radius 5.4cm, speed under 2 m/s) to hole out; land it in water for a 1-stroke penalty and replay from where you teed the shot from.

## Gameplay

水ハザードでボールが止まると +1 打ペナルティでショット開始位置に戻る。
スコアラベル (Birdie/Par 等) はストローク数のみで判定し、ペナルティは Total と vs Par に反映される。

1. **Par select screen** — choose Par 3 / 4 / 5 with the `3` / `4` / `5` key. The Phase 1 synthetic course is tuned for Par 4 (~185m tee-to-pin) but all three are playable.
2. **Shot loop** — aim with `a` / `d` (yaw) and `w` / `s` (pitch), pick a club with `1`-`8`, then:
   - Press `Space` once — the power meter starts oscillating 0 → 100% → 0% on a 2.4s triangle wave.
   - Press `Space` again — whichever value is showing at that instant becomes the shot's strength, and the ball flies.
   - `x` during the power swing cancels back to aim.
3. **Ball at rest** — after the ball stops, the HUD shows its lie (fairway / rough / green / bunker / tee) and recommends the putter on the green. Press `Space` to start the next shot.
4. **Water penalty** — if the ball comes to rest on a water tile, you take a 1-stroke penalty and the ball is teleported back to where you teed this shot from.
5. **Hole out** — once the ball ends inside the cup (5.4cm radius) at under 2 m/s, the round ends. The HUD shows your score (Hole in One / Albatross / Eagle / Birdie / Par / Bogey / …) plus a stroke-by-stroke recap.
6. **Retry or quit** — press `Y` to play the same par again, or `N` / `Esc` to exit.

### Controls

| Key | Phase | Effect |
|---|---|---|
| `3` / `4` / `5` | Par select | Pick par and start playing |
| `1`-`8` | Playing | Club: Driver / 3I / 5I / 7I / 9I / PW / SW / Putter |
| `a` / `d` | Playing | Yaw left / right (~2°) |
| `w` / `s` | Playing | Pitch up / down (0°-45°) |
| `Space` | Playing | Aim → start swing → release → (after rest) back to aim |
| `x` | Playing | Cancel the current swing (PowerSwinging → Aiming) |
| `Y` | Finished | Retry with the same par |
| `N` | Finished | Quit |
| `Esc` | Any | Quit |

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
