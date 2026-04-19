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

## Phase 2 — ball physics wired in

`src/physics.rs` turns `Course::cell_heights` into rapier3d 0.32 colliders. Two invariants drive the build:

1. **Triangulation matches termray's floor renderer.** Every cell `(x, y)` is split into `(NW, NE, SE)` + `(NW, SE, SW)` — the same split termray uses when it rasterises the sloped floor. The `(NW, NE, SE) + (NW, SE, SW)` split matches termray's bilinear surface exactly along the NW→SE diagonal; off-diagonal points deviate by at most the bilinear patch saddle, which is sub-centimetre for Phase 1 terrain (±0.4m amplitude). The deviation is invisible at the ball's 0.021m radius and the 60Hz integration step. Corners use termray's `CornerHeights.floor = [NW, NE, SW, SE]` directly.
2. **Vertical axis is Z.** rapier3d's default is Y-up; street-golf sets gravity to `(0, 0, -9.81)` and constructs all `Vector::new(x, y, z)` with z as height, staying consistent with termray's Z coordinate.

Triangles are grouped by the target tile type (TEE / FAIRWAY / ROUGH / BUNKER / GREEN / WATER), and each group becomes one `ColliderBuilder::trimesh` fixed collider with its own friction / restitution. Wall cells are handled exclusively by a vertical `cuboid(0.5, 0.5, 5.0)` per cell so the ball can't escape through the map border even if it launches high — their z=0 floor triangles would be unreachable behind the cuboid, so the trimesh build skips `TILE_WALL` entirely.

The ball is a 46 g dynamic rigid body (radius 2.1 cm) with CCD enabled. `Physics::step(dt)` runs as many fixed 1/60 s physics ticks as `dt` warrants via an accumulator, leaving the render loop free to run at whatever pace the terminal can sustain. At-rest detection is `|linvel| < 0.05 ∧ |angvel| < 0.1` held for 0.5 s.

`FollowCam` (`src/camera_follow.rs`) derives the termray camera pose from the ball state. The MVP mode `ShotStanding` places the camera 5 m behind the ball (along yaw) and 2 m above, with a small downward pitch. `Flying` and `FirstPerson` exist as enum variants for Phase 3+ but currently fall through to `ShotStanding` to avoid `todo!()` panics in example code.

`examples/shot_test.rs` is the Phase 2 end-to-end demo: 0.5 s after launch it applies `set_linvel((12, 0, 4))` and visualises the flight-to-rest trajectory with the ShotStanding camera.

## Phase 3 — shot input + swing sequence

`src/shot.rs` is the pure-logic input state machine. It is completely independent of crossterm and termray so the tests can exercise every transition without touching a terminal.

### State transitions

```
     [space]                 [space]               physics.at_rest
Aiming ───► PowerSwinging ───────────► Flight ──────────────────► AtRest
  ▲                                                                  │
  └──────────────────────── [space] ─────────────────────────────────┘
```

- **Aiming** — `a/d` adjusts yaw, `w/s` adjusts pitch (0°-45° clamp), `1`-`8` switches club. Power meter is idle.
- **PowerSwinging** — the power value auto-oscillates as a triangle wave (0 → 1 → 0 over `POWER_CYCLE_SEC = 2.4s`). Space snapshots the current value and releases the shot.
- **Flight** — inputs are ignored. `src/main.rs` ticks the physics and waits for `BallState::at_rest` (with a `FLIGHT_REST_HOLD_SEC = 0.5s` guard against the at-rest flag tripping on frame 0 before velocity has ramped up from the impulse).
- **AtRest** — HUD shows the tile the ball came to rest on. If it's on the green, the HUD suggests the putter. Space returns to Aiming for the next shot.

### Club table

| Key | Club | max_speed_mps | loft_deg | Label |
|---|---|---|---|---|
| `1` | Driver | 65.0 | 11.0 | `Driver` |
| `2` | 3 Iron | 60.0 | 21.0 | `3I` |
| `3` | 5 Iron | 55.0 | 26.0 | `5I` |
| `4` | 7 Iron | 50.0 | 33.0 | `7I` |
| `5` | 9 Iron | 43.0 | 40.0 | `9I` |
| `6` | Pitching Wedge | 36.0 | 45.0 | `PW` |
| `7` | Sand Wedge | 28.0 | 54.0 | `SW` |
| `8` | Putter | 18.0 | 0.0 | `Putter` |

Values are MVP-grade approximations — they're tuned so a 3-shot round can clear the 200m synthetic course (Driver → 7I → Putter works on the seed=42 layout).

### Why auto-oscillating power (and not press-hold)

Terminals under crossterm deliver `KeyEvent::Press` reliably, but `KeyEvent::Release` requires the [Kitty keyboard protocol](https://sw.kovidgoyal.net/kitty/keyboard-protocol/) or equivalent, which is not supported by iTerm2 / Terminal.app / most Windows consoles. A classic "hold space to fill, release to fire" meter would work on Kitty / modern Linux consoles only. The triangle-wave oscillator gives the same twitch-timing feel on every terminal with just `KeyPress`: start the swing with Space, release with a second Space when the bar is where you want it.

### Velocity formula

`ShotState::compute_launch_velocity` returns

```
v = speed * (cos(pitch) * cos(yaw),
             cos(pitch) * sin(yaw),
             sin(pitch))
```

with `speed = club.max_speed_mps * power` and `pitch = effective_pitch_deg.to_radians()`. `yaw = 0` points toward +X (pin direction on the synthetic course). `pitch = 0` is a horizontal roll shot; `pitch = 45°` is the high launch.

`effective_pitch_deg` is the club's loft by default. Any `w`/`s` press flips `loft_override = true`, after which subsequent club switches no longer clobber the manual angle — convenient when you want to chip with a wedge at a custom angle, for instance.

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

## Phase 4 — round management

`src/round.rs` ではプレイヤーのラウンドをモデル化する。`Physics` や `ShotState`
には手を入れず、既存のステートを `RoundState` でラップして進行を追いかける
スタイルを取った。

### State machine

```
 ┌──────────────┐  '3'/'4'/'5'  ┌──────────┐  hole out   ┌──────────┐
 │ ParSelect    │ ────────────► │ Playing  │ ──────────► │ Finished │
 └──────────────┘               └──────────┘             └──────────┘
                                     ▲                        │
                                     │     'Y' (retry)        │
                                     └────────────────────────┘
```

- `ParSelect`（開始直後） — `3` / `4` / `5` で Par を選ぶ。`Esc` で終了。
- `Playing` — Phase 3 のショットループがそのまま回る。加えて Flight → AtRest
  遷移のタイミングでカップ判定・水ペナルティ判定・ストローク履歴追加を挟む。
- `Finished` — `Y` でリトライ（Par は維持）、`N` / `Esc` で終了。

### Hole-out check

カップは `Course::pin_world_pos()` で取得した 3D 座標を中心とする円盤で、
水平距離が `CUP_RADIUS_M = 0.054m` 以下（R&A/USGA 規格 4.25in 直径）**かつ**
速度が `HOLE_OUT_SPEED_LIMIT = 2.0 m/s` 未満のとき "ホールイン" と判定する。
2 条件の AND で skim-over（カップを高速で横切る）を除外する。

### Water penalty

水タイル (`TILE_WATER`) で静止した場合は 1 打罰して発射位置
(`round.last_start_pos`) に `Physics::teleport_ball` でボールを戻す。線速度・
角速度はゼロ化するのでそのまま次のショット入力に入れる。ホールインして
いない場合のみ発動する。

### Score label table

| strokes vs par | ラベル |
|---|---|
| `strokes == 1`（Par 問わず） | Hole in One |
| `-3` 以下 | Albatross |
| `-2` | Eagle |
| `-1` | Birdie |
| `0` | Par |
| `+1` | Bogey |
| `+2` | Double Bogey |
| `+3` | Triple Bogey |
| `+4` 以上 | Over |

`vs_par` は `strokes + penalties - par` で計算する（ペナルティも合算）。
