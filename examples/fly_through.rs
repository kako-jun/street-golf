//! Phase 1 の合成コースを歩き回るためのウォークスルー例。
//!
//! `cargo run --release --example fly_through` で起動。`Course::generate(42)` の
//! 200×40 合成コースを termray で描画し、ティーのルーフトップから打ち下ろすビューに
//! 赤い旗竿（ピン）のスプライトが遠方に見える。Phase 2 以降でボール物理と
//! ショット入力が入るまで、地形を確認するためのツール。
//!
//! 操作:
//! - `w` / `s` — 前後移動
//! - `a` / `d` — 左右ストレイフ
//! - `q` / `e` — 左右旋回
//! - `r` / `f` — 上下ピッチ
//! - `esc` — 終了

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
    Course, TILE_BUNKER, TILE_FAIRWAY, TILE_GREEN, TILE_ROUGH, TILE_TEE, TILE_WATER, step_x, step_y,
};
use termray::{
    Camera, Color, FloorTexturer, Framebuffer, HeightMap, HitSide, Sprite, SpriteArt, SpriteDef,
    TILE_WALL, TileType, WallTexturer, project_sprites, render_floor_ceiling, render_sprites,
    render_walls,
};

const MAX_DISTANCE: f64 = 60.0;
const PIN_SPRITE_TYPE: u8 = 1;

/// タイル種に応じて床色を切り替える描画者。`Course` への参照を持たせて、
/// ワールド座標 → タイル ID → 色のマッピングを閉じ込める。
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

struct PinArt {
    def: SpriteDef,
}

impl PinArt {
    fn new() -> Self {
        // 縦長の旗竿。`#` がピンの棒と旗布、`.` が透明。
        // ピンの高さ（height_scale）を大きめにして遠くからも見える。
        const PATTERN: &[&str] = &[
            "##.", "##.", "##.", "#..", "#..", "#..", "#..", "#..", "#..", "#..",
        ];
        Self {
            def: SpriteDef {
                pattern: PATTERN,
                height_scale: 2.0,
                float_offset_scale: 0.0,
            },
        }
    }
}

impl SpriteArt for PinArt {
    fn art(&self, sprite_type: u8) -> Option<&SpriteDef> {
        if sprite_type == PIN_SPRITE_TYPE {
            Some(&self.def)
        } else {
            None
        }
    }

    fn color(&self, _sprite_type: u8) -> Color {
        Color::rgb(220, 30, 30)
    }
}

fn tile_name(t: Option<TileType>) -> &'static str {
    match t {
        Some(TILE_TEE) => "tee",
        Some(TILE_FAIRWAY) => "fairway",
        Some(TILE_ROUGH) => "rough",
        Some(TILE_BUNKER) => "bunker",
        Some(TILE_GREEN) => "green",
        Some(TILE_WATER) => "water",
        Some(TILE_WALL) => "wall",
        Some(_) => "?",
        None => "oob",
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
                Print("▀"),
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

    let course = Course::generate(42);
    let scene = CourseScene { course: &course };
    let pin_art = PinArt::new();

    let (tee_x, tee_y, tee_z) = course.tee_spawn();
    let (pin_x, pin_y) = course.pin();

    let mut cam = Camera::with_z(tee_x, tee_y, tee_z, 0.0, 70f64.to_radians());
    let mut fb = Framebuffer::new(fb_w, fb_h);

    let mut vx = 0.0_f64;
    let mut vy = 0.0_f64;
    let accel = 12.0;
    let friction = 6.0;
    let turn_rate = 2.4;
    let pitch_rate = 1.5;
    let pitch_limit = 0.9;
    let radius = 0.2;

    // 旗竿。Phase 1 では 1 本のみ。
    let sprites = vec![Sprite {
        x: pin_x,
        y: pin_y,
        sprite_type: PIN_SPRITE_TYPE,
    }];

    enable_raw_mode()?;
    execute!(stdout(), EnterAlternateScreen, Hide, Clear(ClearType::All))?;

    let mut last = Instant::now();
    let mut running = true;

    while running {
        let now = Instant::now();
        let dt = (now - last).as_secs_f64().min(0.05);
        last = now;

        let mut dvx = 0.0;
        let mut dvy = 0.0;
        let mut dyaw = 0.0;
        let mut dpitch = 0.0;
        while poll(Duration::from_millis(0))? {
            if let Event::Key(key) = read()? {
                let fwd = cam.forward();
                let right = cam.right();
                match key.code {
                    KeyCode::Esc => {
                        running = false;
                        break;
                    }
                    KeyCode::Char('w') => {
                        dvx += fwd.x * accel * dt;
                        dvy += fwd.y * accel * dt;
                    }
                    KeyCode::Char('s') => {
                        dvx -= fwd.x * accel * dt;
                        dvy -= fwd.y * accel * dt;
                    }
                    KeyCode::Char('d') => {
                        dvx += right.x * accel * dt;
                        dvy += right.y * accel * dt;
                    }
                    KeyCode::Char('a') => {
                        dvx -= right.x * accel * dt;
                        dvy -= right.y * accel * dt;
                    }
                    KeyCode::Char('q') => dyaw -= turn_rate * dt,
                    KeyCode::Char('e') => dyaw += turn_rate * dt,
                    KeyCode::Char('r') => dpitch += pitch_rate * dt,
                    KeyCode::Char('f') => dpitch -= pitch_rate * dt,
                    _ => {}
                }
            }
        }

        vx += dvx;
        vy += dvy;
        let decay = (-friction * dt).exp();
        vx *= decay;
        vy *= decay;

        let (nx, blocked_x) = step_x(&course, cam.x, cam.y, vx * dt, radius);
        let (ny, blocked_y) = step_y(&course, nx, cam.y, vy * dt, radius);
        if blocked_x {
            vx = 0.0;
        }
        if blocked_y {
            vy = 0.0;
        }

        // 足元の地形高さを bilinear でサンプリング。起伏に追従する。
        let cx = nx.floor() as i32;
        let cy = ny.floor() as i32;
        let u = nx - cx as f64;
        let v = ny - cy as f64;
        let floor_h = course.cell_heights(cx, cy).sample_floor(u, v);
        let new_z = floor_h + 0.5;

        let new_yaw = cam.angle + dyaw;
        let new_pitch = (cam.pitch + dpitch).clamp(-pitch_limit, pitch_limit);
        cam.set_pose(nx, ny, new_yaw);
        cam.set_z(new_z);
        cam.set_pitch(new_pitch);

        fb.clear(Color::default());
        let rays = cam.cast_all_rays(&course, fb_w, MAX_DISTANCE);
        render_floor_ceiling(&mut fb, &rays, &scene, &course, &cam, MAX_DISTANCE);
        render_walls(&mut fb, &rays, &scene, &course, &cam, MAX_DISTANCE);

        let projected = project_sprites(&sprites, &cam, &course, fb_w, fb_h);
        render_sprites(&mut fb, &projected, &rays, &pin_art, MAX_DISTANCE);

        let name = tile_name(course.tile_at(cx, cy));
        let status = format!(
            "x={:5.1} y={:5.1} z={:5.2} tile={name} [wasd qe / rf=pitch esc=quit]",
            cam.x, cam.y, cam.z
        );
        present(&fb, &status)?;

        std::thread::sleep(Duration::from_millis(16));
    }

    execute!(stdout(), Show, LeaveAlternateScreen)?;
    disable_raw_mode()?;
    Ok(())
}
