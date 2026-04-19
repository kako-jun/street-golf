//! Phase 2 のショット検証例。
//!
//! `cargo run --release --example shot_test` で起動。0.5 秒後にハードコード
//! された初速 `(12, 0, 4) m/s` をボールに与え、rapier3d の物理で飛翔 →
//! 着地 → 転動 → 停止するまでを 3 人称追従カメラで観察する。Phase 3 で
//! キー入力によるショット操作に置き換える予定。
//!
//! 操作:
//! - `esc` — 終了
//! - 停止後 15 秒経過すると自動終了

use std::io::{Write, stdout};
use std::time::{Duration, Instant};

use crossterm::cursor::{Hide, MoveTo, Show};
use crossterm::event::{Event, KeyCode, poll, read};
use crossterm::execute;
use crossterm::style::{
    Color as CtColor, Print, ResetColor, SetBackgroundColor, SetForegroundColor,
};
use crossterm::terminal::{
    Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode,
    enable_raw_mode, size,
};

use street_golf::{
    Course, FollowCam, FollowMode, Physics, TILE_BUNKER, TILE_FAIRWAY, TILE_GREEN, TILE_ROUGH,
    TILE_TEE, TILE_WATER,
};
use termray::{
    Camera, Color, FloorTexturer, Framebuffer, HitSide, Sprite, SpriteArt, SpriteDef, TILE_WALL,
    TileType, WallTexturer, project_sprites, render_floor_ceiling, render_sprites, render_walls,
};

const MAX_DISTANCE: f64 = 80.0;
const PIN_SPRITE_TYPE: u8 = 1;
const BALL_SPRITE_TYPE: u8 = 2;
const LAUNCH_DELAY: f64 = 0.5;
const POST_REST_TIMEOUT: f64 = 15.0;

struct CourseScene<'a> {
    course: &'a Course,
}

impl<'a> WallTexturer for CourseScene<'a> {
    fn sample_wall(
        &self,
        _tile: TileType,
        _wall_x: f64,
        _wall_y: f64,
        side: HitSide,
        brightness: f64,
        _tile_hash: u32,
    ) -> Color {
        let base = match side {
            HitSide::Vertical => Color::rgb(160, 140, 100),
            HitSide::Horizontal => Color::rgb(130, 110, 80),
        };
        base.darken(brightness)
    }
}

impl<'a> FloorTexturer for CourseScene<'a> {
    fn sample_floor(&self, wx: f64, wy: f64, brightness: f64) -> Color {
        let cx = wx.floor() as i32;
        let cy = wy.floor() as i32;
        let color = match self.course.tile_at(cx, cy) {
            Some(TILE_FAIRWAY) => Color::rgb(100, 160, 80),
            Some(TILE_ROUGH) => Color::rgb(70, 110, 55),
            Some(TILE_BUNKER) => Color::rgb(220, 200, 150),
            Some(TILE_GREEN) => Color::rgb(120, 180, 90),
            Some(TILE_TEE) => Color::rgb(110, 140, 100),
            Some(TILE_WATER) => Color::rgb(50, 100, 180),
            Some(TILE_WALL) => Color::rgb(160, 140, 100),
            _ => Color::rgb(70, 110, 55),
        };
        color.darken(brightness)
    }

    fn sample_ceiling(&self, _wx: f64, _wy: f64, brightness: f64) -> Color {
        Color::rgb(130, 170, 210).darken(brightness)
    }
}

/// ピン（赤い旗竿）とボール（白球）を 1 本の SpriteArt で配信する。
struct ShotArt {
    pin: SpriteDef,
    ball: SpriteDef,
}

impl ShotArt {
    fn new() -> Self {
        const PIN_PATTERN: &[&str] = &[
            "##.", "##.", "##.", "#..", "#..", "#..", "#..", "#..", "#..", "#..",
        ];
        const BALL_PATTERN: &[&str] = &[".#.", "###", ".#."];
        Self {
            pin: SpriteDef {
                pattern: PIN_PATTERN,
                height_scale: 2.0,
                float_offset_scale: 0.0,
            },
            ball: SpriteDef {
                pattern: BALL_PATTERN,
                height_scale: 0.3,
                float_offset_scale: 1.0,
            },
        }
    }
}

impl SpriteArt for ShotArt {
    fn art(&self, sprite_type: u8) -> Option<&SpriteDef> {
        match sprite_type {
            PIN_SPRITE_TYPE => Some(&self.pin),
            BALL_SPRITE_TYPE => Some(&self.ball),
            _ => None,
        }
    }

    fn color(&self, sprite_type: u8) -> Color {
        match sprite_type {
            PIN_SPRITE_TYPE => Color::rgb(220, 30, 30),
            BALL_SPRITE_TYPE => Color::rgb(240, 240, 240),
            _ => Color::rgb(255, 255, 255),
        }
    }
}

struct TerminalGuard;

impl TerminalGuard {
    fn new() -> std::io::Result<Self> {
        enable_raw_mode()?;
        execute!(stdout(), EnterAlternateScreen, Hide, Clear(ClearType::All))?;
        Ok(Self)
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = execute!(stdout(), Show, LeaveAlternateScreen);
        let _ = disable_raw_mode();
    }
}

fn present(fb: &Framebuffer, status: &str) -> std::io::Result<()> {
    let mut stdout = stdout();
    execute!(stdout, MoveTo(0, 0))?;
    for y in (0..fb.height()).step_by(2) {
        for x in 0..fb.width() {
            let top = fb.get_pixel(x, y);
            let bot = fb.get_pixel(x, (y + 1).min(fb.height() - 1));
            execute!(
                stdout,
                SetForegroundColor(CtColor::Rgb {
                    r: top.r,
                    g: top.g,
                    b: top.b
                }),
                SetBackgroundColor(CtColor::Rgb {
                    r: bot.r,
                    g: bot.g,
                    b: bot.b
                }),
                Print("\u{2580}"),
            )?;
        }
        execute!(stdout, ResetColor, Print("\r\n"))?;
    }
    execute!(stdout, ResetColor, Print(status))?;
    stdout.flush()
}

fn main() -> std::io::Result<()> {
    let (cols, rows) = size()?;
    let fb_w = cols as usize;
    let fb_h = (rows as usize).saturating_sub(2) * 2;
    if fb_w == 0 || fb_h == 0 {
        return Ok(());
    }

    let course = Course::generate(42);
    let scene = CourseScene { course: &course };
    let art = ShotArt::new();

    let (tee_x, tee_y, tee_z) = course.tee_spawn();
    let (pin_x, pin_y) = course.pin();
    let ball_spawn = [tee_x + 0.5, tee_y, tee_z - 0.5 + 0.3];

    let mut physics = Physics::new(&course, ball_spawn);
    let yaw = 0.0;
    let follow = FollowCam::new(FollowMode::ShotStanding, yaw);

    let mut cam = Camera::with_z(tee_x, tee_y, tee_z, yaw, 70f64.to_radians());
    let mut fb = Framebuffer::new(fb_w, fb_h);

    let _guard = TerminalGuard::new()?;

    let mut last = Instant::now();
    let mut elapsed = 0.0_f64;
    let mut launched = false;
    let mut rest_timer = 0.0_f64;
    let mut running = true;

    while running {
        let now = Instant::now();
        let dt = (now - last).as_secs_f64().min(0.05);
        last = now;
        elapsed += dt;

        while poll(Duration::from_millis(0))? {
            if let Event::Key(key) = read()?
                && key.code == KeyCode::Esc
            {
                running = false;
                break;
            }
        }

        if !launched && elapsed >= LAUNCH_DELAY {
            physics.launch([12.0, 0.0, 4.0]);
            launched = true;
        }

        physics.step(dt);
        let state = physics.ball_state();
        if state.at_rest {
            rest_timer += dt;
            if rest_timer >= POST_REST_TIMEOUT {
                running = false;
            }
        } else {
            rest_timer = 0.0;
        }

        let (cx, cy, cz, cyaw, cpitch) = follow.update(state.pos);
        cam.set_pose(cx, cy, cyaw);
        cam.set_z(cz);
        cam.set_pitch(cpitch);

        let sprites = vec![
            Sprite {
                x: pin_x,
                y: pin_y,
                sprite_type: PIN_SPRITE_TYPE,
            },
            Sprite {
                x: state.pos[0],
                y: state.pos[1],
                sprite_type: BALL_SPRITE_TYPE,
            },
        ];

        fb.clear(Color::default());
        let rays = cam.cast_all_rays(&course, fb_w, MAX_DISTANCE);
        render_floor_ceiling(&mut fb, &rays, &scene, &course, &cam, MAX_DISTANCE);
        render_walls(&mut fb, &rays, &scene, &course, &cam, MAX_DISTANCE);
        let projected = project_sprites(&sprites, &cam, &course, fb_w, fb_h);
        render_sprites(&mut fb, &projected, &rays, &art, MAX_DISTANCE);

        let status = format!(
            "pos=({:5.1},{:5.1},{:5.2}) vel=({:5.1},{:5.1},{:5.1}) rest={} [esc=quit]",
            state.pos[0],
            state.pos[1],
            state.pos[2],
            state.vel[0],
            state.vel[1],
            state.vel[2],
            state.at_rest
        );
        present(&fb, &status)?;

        std::thread::sleep(Duration::from_millis(16));
    }

    Ok(())
}
