# street-golf — Architecture

> Phase 0 document. Expect this file to grow as each phase lands.

## Three-layer model

street-golf is a thin orchestrator over two external crates: `rapier3d` for ball physics and `termray` for raycasted rendering. The runtime splits cleanly into three layers.

```
+-------------------------------------------------------------+
|  rapier3d (physics)        (src/physics.rs, Phase 2)        |
|  - ball as dynamic rigid body                               |
|  - course surface as static heightfield / trimesh collider  |
|  - integrate each frame, read back ball transform           |
+-------------------------------------------------------------+
|  termray (raycaster)       (src/course.rs, Phase 1)         |
|  - HeightMap implementation for sloped ground               |
|  - sprite for the ball, text label for the pin              |
|  - Framebuffer written per frame                            |
+-------------------------------------------------------------+
|  crossterm (terminal I/O)  (src/main.rs, src/shot.rs)       |
|  - alternate screen + raw mode                              |
|  - half-block Framebuffer → terminal presentation           |
|  - key events → shot / navigation state                     |
+-------------------------------------------------------------+
```

## Data flow (Phase 2+)

```
user input ──► shot parameters (club, aim, power)
                    │
                    ▼
            rapier3d integration
            (ball position / velocity per tick)
                    │
                    ▼
         camera pose derived from ball state
                    │
                    ▼
          termray renders the framebuffer
          (course surface + ball sprite + pin label)
                    │
                    ▼
           crossterm presents half-block output
```

## termray capability requirements

| Capability | termray version | Used by |
|---|---|---|
| Sloped floors / HeightMap | 0.3.0 | `course.rs` (Phase 1) |
| Sprites | 0.2.0 | Phase 1 example (`fly_through` ピン), Phase 2+ 物理 ↔ renderer glue |
| Text labels above sprites | 0.3.0 | `course.rs` (pin), `hud.rs` (Phase 4) |

All three have already shipped in termray 0.3, so street-golf can depend on a stable release line from Phase 1 onward without patching the upstream engine.

## Phase dependency graph

```
Phase 0 (#1)  skeleton            ─┐
                                   ├─► Phase 1 (#2) synthetic course
Phase 1 (#2)  Course heightmap ───┬┘
                                  │
                                  ├─► Phase 2 (#3) ball physics ──┐
                                  │                                │
                                  └─► Phase 3 (#4) shot input ─────┤
                                                                   ▼
                                                        Phase 4 (#5) score + round end
                                                                   │
                                                                   ▼
                                                        Phase 5+ OSM/SRTM import
```

- **Phase 0** lays down the build, CI, release, and render pipeline plumbing. `rapier3d` is declared as a dependency but not yet `use`d — Phase 2 picks it up.
- **Phase 1** introduces `course.rs` — a `Course` type that implements termray's `HeightMap` trait and lays out tee, fairway, green, and hole for a single synthetic hole.
- **Phase 2** wires `rapier3d` in `physics.rs`: the course becomes a static collider, the ball becomes a dynamic rigid body, and ball transform feeds the termray camera each frame.
- **Phase 3** adds `shot.rs`: club selection, aim, power, and the shot-to-rest sequence that steps physics until the ball settles.
- **Phase 4** adds `hud.rs`: score card, club indicator, hole-out detection, round end.
- **Phase 5+** replaces the synthetic course with real-world data pulled from OpenStreetMap (geometry) and SRTM (elevation).

## Current state (Phase 1)

`src/course.rs` now defines `Course` — a synthetic 1-hole course that implements both `termray::TileMap` and `termray::HeightMap`. `examples/fly_through.rs` drives it as an interactive walkthrough so the terrain can be verified visually before Phase 2 introduces ball physics.

`src/main.rs` still runs the Phase 0 splash and stays that way until Phase 2. The Phase 1 entry point is the example:

```bash
cargo run --release --example fly_through
```

### Map layout

World coordinates: 1 tile = 1m. Map is 200 × 40 tiles; world origin `(0, 0)` is the NW corner, `y` increases southward.

| Region | Tile | Extent (inclusive) | Surface height |
|---|---|---|---|
| Outer border | `TILE_WALL` (1) | x ∈ {0, 199} ∪ y ∈ {0, 39} | 0.0m (not rendered from inside) |
| Tee | `TILE_TEE` (3) | x=3..6, y=18..21 | flat 20.0m rooftop |
| Fairway | `TILE_FAIRWAY` (4) | x=10..179, y=15..24 | `base_height` (±0.4m) |
| Rough | `TILE_ROUGH` (5) | fill elsewhere inside the border | `base_height` (±0.4m) |
| Bunker 1 | `TILE_BUNKER` (6) | x=80..83, y=19..21 | `base_height − 1.5m` |
| Bunker 2 | `TILE_BUNKER` (6) | x=140..142, y=17..20 | `base_height − 1.5m` |
| Water | `TILE_WATER` (8) | x=105..109, y=17..21 | −0.2m, **solid** |
| Green | `TILE_GREEN` (7) | x=180..194, y=16..23 | `base_height * 0.3 − 0.3 * exp(−d² / 6.0)` where `d` = distance to pin |
| Pin (sprite) | — | `(190.5, 20.5)` | — |
| Tee spawn (camera) | — | `(5.0, 20.0, 20.5)` | z = rooftop + 0.5m eye |

`base_height(cx, cy) = 0.4 * amp_scale * (0.6 * sin(0.07·cx + phase_x) + 0.4 * sin(0.11·cy + phase_y))`, where `phase_x`, `phase_y`, `amp_scale` are sampled from `StdRng::seed_from_u64(seed)`. `Course::generate(seed)` is deterministic — same seed always yields identical per-corner height arrays.

### Corner priority

The 201 × 41 corner grid is shared between adjacent tiles, so each corner is classified by **the highest-priority tile among its four neighboring cells** (NW / NE / SW / SE). Priority (highest first):

1. `TILE_WALL` — forces `floor = 0.0`
2. `TILE_TEE` — forces `floor = 20.0` (guarantees the 4×4 tee is strictly flat)
3. `TILE_BUNKER`
4. `TILE_WATER`
5. `TILE_GREEN`
6. `TILE_FAIRWAY`
7. `TILE_ROUGH`

`ceil[corner] = floor[corner] + 3.0m` everywhere.

### is_solid contract

`TILE_WALL`, `TILE_VOID`, and `TILE_WATER` are solid. Out-of-bounds tiles return `true` (per `TileMap` contract). Tee / fairway / rough / bunker / green are walkable.

## External dependencies

| Crate | Version | Role |
|---|---|---|
| [`termray`](https://crates.io/crates/termray) | 0.3 | Raycasting, framebuffer, sprite/label projection |
| [`rapier3d`](https://crates.io/crates/rapier3d) | 0.32 | 3D rigid-body physics for the ball (Phase 2+) |
| [`crossterm`](https://crates.io/crates/crossterm) | 0.29 | Cross-platform terminal I/O and input events |
| [`rand`](https://crates.io/crates/rand) | 0.9 | Course-layout randomness, wind (Phase 1+) |
| [`anyhow`](https://crates.io/crates/anyhow) | 1 | Error propagation in the binary |

The dependency list is deliberately minimal. Phase 5+ will add OSM / SRTM parsers when real-world data import begins.
