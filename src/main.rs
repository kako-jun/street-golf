//! street-golf — Phase 3 のプレイアブルゲームループ。
//!
//! ティーからピンまでの 1 ホールを、クラブ選択 + 狙い + パワーの入力で
//! 何打かに分けて攻める。crossterm でキー入力を取り、[`ShotState`] と
//! [`Physics`] を駆動し、termray の半ブロック描画で毎フレームを present する。
//! ホールアウト判定とスコア確定は Phase 4 (#5) で追加予定。

use std::io::{Write, stdout};
use std::time::{Duration, Instant};

use anyhow::Result;
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
    shot::{Club, PITCH_STEP_DEG, ShotPhase, ShotState, YAW_STEP_RAD},
};
use termray::{
    Camera, Color, FloorTexturer, Framebuffer, HitSide, Sprite, SpriteArt, SpriteDef, TILE_WALL,
    TileType, WallTexturer, project_sprites, render_floor_ceiling, render_sprites, render_walls,
};

const MAX_DISTANCE: f64 = 60.0;
const PIN_SPRITE_TYPE: u8 = 1;
const BALL_SPRITE_TYPE: u8 = 2;
/// Flight 開始直後は `Physics` 側の at_rest タイマーが立ち上がる前なので、
/// このホールド時間を過ぎるまでは rest 判定を信用しない。
const FLIGHT_REST_HOLD_SEC: f64 = 0.5;
/// HUD に予約する行数。
const HUD_ROWS: usize = 3;
/// メインループのターゲット fps (≈60Hz)。
const FRAME_SLEEP: Duration = Duration::from_millis(16);

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

struct GolfArt {
    pin: SpriteDef,
    ball: SpriteDef,
}

impl GolfArt {
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

impl SpriteArt for GolfArt {
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
    fn new() -> Result<Self> {
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

/// 入力ハンドラの返り値。`Launch` は `Physics::launch` を呼ぶタイミング。
enum Action {
    Continue,
    Quit,
    Launch([f64; 3]),
}

fn handle_key(shot: &mut ShotState, code: KeyCode) -> Action {
    match code {
        KeyCode::Esc => Action::Quit,
        KeyCode::Char('q') if shot.phase == ShotPhase::Aiming => {
            // Aiming 中の 'q' は終了ショートカット。将来 spin を割り当てるまでの暫定措置。
            Action::Quit
        }
        KeyCode::Char(' ') => {
            let launched = shot.press_space();
            if launched {
                Action::Launch(shot.compute_launch_velocity())
            } else {
                Action::Continue
            }
        }
        KeyCode::Char('a') => {
            shot.adjust_yaw(-YAW_STEP_RAD);
            Action::Continue
        }
        KeyCode::Char('d') => {
            shot.adjust_yaw(YAW_STEP_RAD);
            Action::Continue
        }
        KeyCode::Char('w') => {
            shot.adjust_pitch(PITCH_STEP_DEG);
            Action::Continue
        }
        KeyCode::Char('s') => {
            shot.adjust_pitch(-PITCH_STEP_DEG);
            Action::Continue
        }
        KeyCode::Char(c) => {
            if let Some(club) = Club::from_digit(c) {
                shot.select_club(club);
            }
            Action::Continue
        }
        _ => Action::Continue,
    }
}

fn tile_name(tile: Option<TileType>) -> &'static str {
    match tile {
        Some(TILE_TEE) => "tee",
        Some(TILE_FAIRWAY) => "fairway",
        Some(TILE_ROUGH) => "rough",
        Some(TILE_BUNKER) => "bunker",
        Some(TILE_GREEN) => "green",
        Some(TILE_WATER) => "water",
        Some(TILE_WALL) => "wall",
        _ => "off",
    }
}

fn hud(
    shot: &ShotState,
    ball_pos: [f64; 3],
    ball_vel: [f64; 3],
    course: &Course,
    width: usize,
) -> String {
    let cx = ball_pos[0].floor() as i32;
    let cy = ball_pos[1].floor() as i32;
    let tile = course.tile_at(cx, cy);
    let tile_label = tile_name(tile);
    let spec = shot.club.spec();
    let yaw_deg = shot.yaw.to_degrees();
    let pitch_deg = shot.effective_pitch_deg();
    let line1 = format!(
        "Club: {} (loft {:.0}°, max {:.0}m/s) | Shots: {} | Yaw: {:+.1}° Pitch: {:.1}° | Tile: {}",
        spec.label, spec.loft_deg, spec.max_speed_mps, shot.shots, yaw_deg, pitch_deg, tile_label,
    );
    let line2 = match shot.phase {
        ShotPhase::Aiming => "Power: -- ready -- (space to begin swing)".to_string(),
        ShotPhase::PowerSwinging => {
            let bar_w = 20usize;
            let filled = (shot.power * bar_w as f64).round() as usize;
            let filled = filled.min(bar_w);
            let bar: String = std::iter::repeat_n('#', filled)
                .chain(std::iter::repeat_n('.', bar_w - filled))
                .collect();
            format!(
                "Power: [{}] {:>3}%  (space to release)",
                bar,
                (shot.power * 100.0).round() as i32,
            )
        }
        ShotPhase::Flight => format!(
            "Flight... ball ({:5.1},{:5.1},{:5.2}) vel ({:5.1},{:5.1},{:5.1}) m/s",
            ball_pos[0], ball_pos[1], ball_pos[2], ball_vel[0], ball_vel[1], ball_vel[2],
        ),
        ShotPhase::AtRest => {
            let suffix = if tile == Some(TILE_GREEN) {
                " [Putter recommended]"
            } else {
                ""
            };
            format!(
                "-- Ball at rest on {}. Space for next shot.{}",
                tile_label, suffix,
            )
        }
    };
    let line3 = "Keys: 1-8=club  a/d=yaw  w/s=pitch  space=shoot  esc=quit".to_string();
    let mut out = String::new();
    for (i, line) in [line1, line2, line3].iter().enumerate() {
        let mut s = line.clone();
        if s.chars().count() > width {
            s = s.chars().take(width).collect();
        } else {
            let pad = width.saturating_sub(s.chars().count());
            for _ in 0..pad {
                s.push(' ');
            }
        }
        out.push_str(&s);
        if i + 1 < 3 {
            out.push_str("\r\n");
        }
    }
    out
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

fn main() -> Result<()> {
    let (cols, rows) = size()?;
    let fb_w = cols as usize;
    let fb_h = (rows as usize).saturating_sub(HUD_ROWS) * 2;
    if fb_w == 0 || fb_h == 0 {
        return Ok(());
    }

    let course = Course::generate(42);
    let scene = CourseScene { course: &course };
    let art = GolfArt::new();

    let (tee_x, tee_y, tee_z) = course.tee_spawn();
    let (pin_x, pin_y) = course.pin();
    let ball_spawn = [tee_x + 0.5, tee_y, tee_z - 0.5 + 0.3];

    let mut physics = Physics::new(&course, ball_spawn);
    let mut shot = ShotState::new(0.0);
    let mut follow = FollowCam::new(FollowMode::ShotStanding, 0.0);

    let mut cam = Camera::with_z(tee_x, tee_y, tee_z, 0.0, 70f64.to_radians());
    let mut fb = Framebuffer::new(fb_w, fb_h);

    let _guard = TerminalGuard::new()?;

    let mut last = Instant::now();
    let mut flight_timer = 0.0_f64;

    loop {
        let now = Instant::now();
        let dt = (now - last).as_secs_f64().min(0.05);
        last = now;

        while poll(Duration::from_millis(0))? {
            if let Event::Key(key) = read()? {
                match handle_key(&mut shot, key.code) {
                    Action::Continue => {}
                    Action::Quit => return Ok(()),
                    Action::Launch(v) => {
                        physics.launch(v);
                        flight_timer = 0.0;
                    }
                }
            }
        }

        shot.tick(dt);
        physics.step(dt);

        let ball = physics.ball_state();
        if shot.phase == ShotPhase::Flight {
            flight_timer += dt;
            if ball.at_rest && flight_timer > FLIGHT_REST_HOLD_SEC {
                shot.notify_rest();
                flight_timer = 0.0;
            }
        }

        follow.yaw = shot.yaw;
        let (ccx, ccy, ccz, cyaw, cpitch) = follow.update(ball.pos);
        cam.set_pose(ccx, ccy, cyaw);
        cam.set_z(ccz);
        cam.set_pitch(cpitch);

        fb.clear(Color::default());
        let rays = cam.cast_all_rays(&course, fb_w, MAX_DISTANCE);
        render_floor_ceiling(&mut fb, &rays, &scene, &course, &cam, MAX_DISTANCE);
        render_walls(&mut fb, &rays, &scene, &course, &cam, MAX_DISTANCE);
        let sprites = [
            Sprite {
                x: pin_x,
                y: pin_y,
                sprite_type: PIN_SPRITE_TYPE,
            },
            Sprite {
                x: ball.pos[0],
                y: ball.pos[1],
                sprite_type: BALL_SPRITE_TYPE,
            },
        ];
        let projected = project_sprites(&sprites, &cam, &course, fb_w, fb_h);
        render_sprites(&mut fb, &projected, &rays, &art, MAX_DISTANCE);

        present(&fb, &hud(&shot, ball.pos, ball.vel, &course, fb_w))?;
        std::thread::sleep(FRAME_SLEEP);
    }
}
