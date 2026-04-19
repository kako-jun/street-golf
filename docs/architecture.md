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
| Sprites | 0.2.0 | `physics.rs` ↔ renderer glue (Phase 2) |
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

## Current state (Phase 0)

`src/main.rs` currently:

1. Reads terminal size via `crossterm::terminal::size`.
2. Allocates a `termray::Framebuffer` at full terminal width × 2×(rows−2) pixels (half-block rendering doubles vertical resolution).
3. Clears it to a dark meadow green.
4. Enters the alternate screen, hides the cursor, enables raw mode.
5. Writes one frame using half-block characters (`▀`): top pixel → foreground RGB, bottom pixel → background RGB.
6. Sleeps ~800 ms so the user can see the blank frame.
7. Restores the terminal and exits.

There is no course, no ball, no input handling yet — those arrive with the phases above.

## External dependencies

| Crate | Version | Role |
|---|---|---|
| [`termray`](https://crates.io/crates/termray) | 0.3 | Raycasting, framebuffer, sprite/label projection |
| [`rapier3d`](https://crates.io/crates/rapier3d) | 0.32 | 3D rigid-body physics for the ball (Phase 2+) |
| [`crossterm`](https://crates.io/crates/crossterm) | 0.29 | Cross-platform terminal I/O and input events |
| [`rand`](https://crates.io/crates/rand) | 0.9 | Course-layout randomness, wind (Phase 1+) |
| [`anyhow`](https://crates.io/crates/anyhow) | 1 | Error propagation in the binary |

The dependency list is deliberately minimal. Phase 5+ will add OSM / SRTM parsers when real-world data import begins.
