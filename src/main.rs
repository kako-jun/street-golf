//! street-golf — Phase 4 のプレイアブルゲームループ。
//!
//! Par 選択（`ParSelect`）→ ショットループ（`Playing`）→ 結果画面（`Finished`）
//! の 3 フェーズを crossterm 入力 + [`RoundState`] で駆動する。`Playing` 中の
//! ショット確定→物理ステップ→追従カメラ→描画→HUD の流れは Phase 3 と同じ
//! で、加えて at-rest 遷移時にカップ判定と水ペナルティを挟み、ホールイン
//! すれば `Finished` に進んで最終スコアを表示する。

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
    Course, FollowCam, FollowMode, Physics, RoundPhase, RoundResult, RoundState, StrokeRecord,
    TILE_BUNKER, TILE_FAIRWAY, TILE_GREEN, TILE_ROUGH, TILE_TEE, TILE_WATER, check_hole_out,
    shot::{Club, PITCH_STEP_DEG, ShotPhase, ShotState, YAW_STEP_RAD},
};
use termray::{
    Camera, Color, FloorTexturer, Framebuffer, HitSide, Sprite, SpriteArt, SpriteDef, TILE_WALL,
    TileType, WallTexturer, project_sprites, render_floor_ceiling, render_sprites, render_walls,
};

const MAX_DISTANCE: f64 = 60.0;
const PIN_SPRITE_TYPE: u8 = 1;
const BALL_SPRITE_TYPE: u8 = 2;
/// Flight 開始直後は `at_rest=true` が 1 フレームだけ立つことがあるので、
/// launch からこの秒数は `notify_rest` を抑制する（発射直後の
/// 速度ランプアップ中に誤判定で AtRest に落ちるのを防ぐ）。
/// `Physics::AT_REST_DURATION` (0.5s) と合わせて、launch から AtRest まで
/// 最低 1 秒未満は絶対にならない。片方を上げるときはもう片方も再考すること。
const FLIGHT_REST_HOLD_SEC: f64 = 0.5;
/// HUD と End ダイアログ共通の高さ。End ダイアログが 8 行（ヘッダ + 最大 5
/// 履歴 + フッタ）を必要とするため、ゲーム中の Playing HUD もそれに合わせる。
const HUD_ROWS: usize = 8;
/// メインループのターゲット fps (≈60Hz)。
const FRAME_SLEEP: Duration = Duration::from_millis(16);
/// `course.tee_spawn()` が tee 面に足すアイレベル（カメラ視点の高さ）。
const TEE_EYE_HEIGHT: f64 = 0.5;
/// ボールを tee 面から何 m 上にスポーンさせるか。
const BALL_DROP_HEIGHT: f64 = 0.3;

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

/// 入力ハンドラの返り値（Playing 用）。`Launch` は `Physics::launch` を呼ぶ
/// タイミング。発射位置を `RoundState::start_stroke` に渡すため呼び出し側
/// で `ball_state` を参照する。
#[must_use]
enum PlayAction {
    Continue,
    Quit,
    Launch([f64; 3]),
}

fn handle_play_key(shot: &mut ShotState, code: KeyCode) -> PlayAction {
    // `q` / `e` は将来のスピン入力 (backspin / topspin) に予約。終了は Esc のみ。
    match code {
        KeyCode::Esc => PlayAction::Quit,
        KeyCode::Char(' ') => {
            let launched = shot.press_space();
            if launched {
                PlayAction::Launch(shot.compute_launch_velocity())
            } else {
                PlayAction::Continue
            }
        }
        KeyCode::Char('a') => {
            shot.adjust_yaw(-YAW_STEP_RAD);
            PlayAction::Continue
        }
        KeyCode::Char('d') => {
            shot.adjust_yaw(YAW_STEP_RAD);
            PlayAction::Continue
        }
        KeyCode::Char('w') => {
            shot.adjust_pitch(PITCH_STEP_DEG);
            PlayAction::Continue
        }
        KeyCode::Char('s') => {
            shot.adjust_pitch(-PITCH_STEP_DEG);
            PlayAction::Continue
        }
        KeyCode::Char('x') => {
            shot.press_cancel();
            PlayAction::Continue
        }
        KeyCode::Char(c) => {
            // '3'..='5' は Par 選択画面でも使うが、Playing 中はクラブ切替として処理される。
            if let Some(club) = Club::from_digit(c) {
                shot.select_club(club);
            }
            PlayAction::Continue
        }
        _ => PlayAction::Continue,
    }
}

/// ParSelect フェーズで受け付けるキー。`Some(par)` で Par 確定、`None` で
/// 入力継続、`Err` で終了要求。
fn handle_par_select_key(code: KeyCode) -> ParAction {
    match code {
        KeyCode::Esc => ParAction::Quit,
        KeyCode::Char(c @ ('3' | '4' | '5')) => ParAction::Select(c.to_digit(10).unwrap()),
        _ => ParAction::Continue,
    }
}

#[must_use]
enum ParAction {
    Continue,
    Quit,
    Select(u32),
}

/// Finished フェーズで受け付けるキー。
fn handle_end_key(code: KeyCode) -> EndAction {
    match code {
        KeyCode::Esc | KeyCode::Char('n') | KeyCode::Char('N') => EndAction::Quit,
        KeyCode::Char('y') | KeyCode::Char('Y') => EndAction::Retry,
        _ => EndAction::Continue,
    }
}

#[must_use]
enum EndAction {
    Continue,
    Quit,
    Retry,
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

/// Par 選択画面の HUD 文字列。
fn par_select_hud(width: usize) -> String {
    let lines = [
        "========== street-golf ==========".to_string(),
        "Course: Phase 1 synthetic (seed 42)".to_string(),
        "Select par:".to_string(),
        "  [3] Par 3   (~130m)".to_string(),
        "  [4] Par 4   (~185m)  <-- recommended".to_string(),
        "  [5] Par 5   (~220m)".to_string(),
        "Esc: Quit".to_string(),
        String::new(),
    ];
    pad_lines(&lines, width)
}

/// Playing フェーズの HUD 文字列。`HUD_ROWS=8` で Par 選択画面 / プレイ中 /
/// End 画面すべて同じ行数に揃える。Playing は 4 本のコンテンツ行 + 4 本の
/// パディング、End は最大 5 本のストローク履歴 + ヘッダ + フッタ = 8。
fn play_hud(
    shot: &ShotState,
    round: &RoundState,
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
    let total = round.strokes + round.penalties;
    let line0 = format!(
        "Par {} | Strokes: {} | Penalties: {} | Total: {}",
        round.par, round.strokes, round.penalties, total,
    );
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
    let line3 = "Keys: 1-8=club  a/d=yaw  w/s=pitch  space=shoot  x=cancel  esc=quit".to_string();
    let lines = [
        line0,
        line1,
        line2,
        line3,
        String::new(),
        String::new(),
        String::new(),
        String::new(),
    ];
    pad_lines(&lines, width)
}

/// Finished フェーズの HUD 文字列。結果サマリ + ストローク履歴 + 操作案内。
fn end_hud(result: &RoundResult, log: &[StrokeRecord], width: usize) -> String {
    let sign = if result.vs_par > 0 {
        format!("+{}", result.vs_par)
    } else {
        result.vs_par.to_string()
    };
    let line0 = "========== Round Complete ==========".to_string();
    // フォーマット方針: "Hole in {total} strokes" で打数とペナルティ込みの合計を
    // 冒頭に出す。スコアラベル (Birdie/Par 等) はストローク数のみで判定し、
    // カッコ内の vs Par はペナルティ込みの合計と Par の差分なので、両者が
    // 食い違うケース（例: Birdie (+1)）はペナルティが 1 以上のときに発生する。
    // フッタの "Label based on strokes only" 行で注記する。
    let line1 = format!(
        "Hole in {} strokes!  Par {} -> {} ({}) [Penalties: {}, Total: {}]",
        result.total, result.par, result.label, sign, result.penalties, result.total,
    );
    // ストローク履歴は最大 5 行まで（それ以上は省略）。
    let max_log_rows = 5;
    let mut log_lines: Vec<String> = log
        .iter()
        .take(max_log_rows)
        .map(|s| {
            let spec = s.club.spec();
            let rest = tile_name(s.rest_tile);
            let pen = if s.penalty { "  [WATER +1]" } else { "" };
            format!(
                "  {}: {:<6} power {:>3}%  -> {}{}",
                s.index,
                spec.label,
                (s.power * 100.0).round() as i32,
                rest,
                pen,
            )
        })
        .collect();
    if log.len() > max_log_rows {
        log_lines.push(format!("  ... (+{} more)", log.len() - max_log_rows));
    }
    // Label は strokes のみで判定するため、Penalties 付きの vs Par と Label が
    // 食い違うことがある（例: Birdie (+1)）。プレイヤー向けに注記する。
    let footer =
        "[Y] Retry  [N/Esc] Quit  (Label based on strokes only; penalties excluded)".to_string();

    let mut lines: Vec<String> = Vec::with_capacity(HUD_ROWS);
    lines.push(line0);
    lines.push(line1);
    lines.extend(log_lines);
    while lines.len() < HUD_ROWS - 1 {
        lines.push(String::new());
    }
    lines.push(footer);
    while lines.len() < HUD_ROWS {
        lines.push(String::new());
    }
    lines.truncate(HUD_ROWS);
    pad_lines(&lines, width)
}

/// 行配列を width で truncate / pad し、`\r\n` で連結する（末尾改行なし）。
fn pad_lines(lines: &[String], width: usize) -> String {
    let mut out = String::new();
    for (i, line) in lines.iter().enumerate() {
        let mut s = line.clone();
        // HUD 文字列は ASCII 前提。日本語タイル名が混ざる場合は unicode-width を導入すること。
        if s.chars().count() > width {
            s = s.chars().take(width).collect();
        } else {
            let pad = width.saturating_sub(s.chars().count());
            for _ in 0..pad {
                s.push(' ');
            }
        }
        out.push_str(&s);
        if i + 1 < lines.len() {
            out.push_str("\r\n");
        }
    }
    out
}

/// Finished フェーズ用に framebuffer 中央を薄暗くする（ダイアログ背景）。
fn dim_center(fb: &mut Framebuffer) {
    let w = fb.width();
    let h = fb.height();
    if w == 0 || h == 0 {
        return;
    }
    let box_w = (w * 6 / 10).min(w);
    let box_h = (h * 6 / 10).min(h);
    let x0 = w.saturating_sub(box_w) / 2;
    let y0 = h.saturating_sub(box_h) / 2;
    for y in y0..(y0 + box_h).min(h) {
        for x in x0..(x0 + box_w).min(w) {
            let p = fb.get_pixel(x, y);
            // 暗めにブレンド（40% を残す）。
            fb.set_pixel(
                x,
                y,
                Color::rgb(
                    (p.r as u32 * 40 / 100) as u8,
                    (p.g as u32 * 40 / 100) as u8,
                    (p.b as u32 * 40 / 100) as u8,
                ),
            );
        }
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
    // tee_z からアイレベル分を引いて足元に戻し、そこから BALL_DROP_HEIGHT だけ上に置く。
    let ball_spawn = [
        tee_x + 0.5,
        tee_y,
        tee_z - TEE_EYE_HEIGHT + BALL_DROP_HEIGHT,
    ];

    let mut physics = Physics::new(&course, ball_spawn);
    let mut shot = ShotState::new(0.0);
    let mut follow = FollowCam::new(FollowMode::ShotStanding, 0.0);
    let mut round = RoundState::new(ball_spawn);

    let mut cam = Camera::with_z(tee_x, tee_y, tee_z, 0.0, 70f64.to_radians());
    let mut fb = Framebuffer::new(fb_w, fb_h);

    let _guard = TerminalGuard::new()?;

    let mut last = Instant::now();
    let mut flight_timer = 0.0_f64;

    loop {
        let now = Instant::now();
        let dt = (now - last).as_secs_f64().min(0.05);
        last = now;

        // --- 入力処理（フェーズ別）---
        while poll(Duration::from_millis(0))? {
            if let Event::Key(key) = read()? {
                match round.phase {
                    RoundPhase::ParSelect => match handle_par_select_key(key.code) {
                        ParAction::Continue => {}
                        ParAction::Quit => return Ok(()),
                        ParAction::Select(p) => round.select_par(p),
                    },
                    RoundPhase::Playing => match handle_play_key(&mut shot, key.code) {
                        PlayAction::Continue => {}
                        PlayAction::Quit => return Ok(()),
                        PlayAction::Launch(v) => {
                            let ball_now = physics.ball_state();
                            round.start_stroke(ball_now.pos);
                            physics.launch(v);
                            flight_timer = 0.0;
                        }
                    },
                    RoundPhase::Finished => match handle_end_key(key.code) {
                        EndAction::Continue => {}
                        EndAction::Quit => return Ok(()),
                        EndAction::Retry => {
                            physics = Physics::new(&course, ball_spawn);
                            shot = ShotState::new(0.0);
                            // shot.yaw も 0.0 に戻るので yaw 0.0 を渡して問題ない。
                            follow = FollowCam::new(FollowMode::ShotStanding, 0.0);
                            round.retry(ball_spawn);
                            flight_timer = 0.0;
                        }
                    },
                }
            }
        }

        // --- 物理・状態更新 ---
        shot.tick(dt);
        // Finished はボールが完全停止しているので物理ステップをスキップする。
        // ball_state() は状態を読むだけなのでカメラ追従は引き続き機能する。
        if !matches!(round.phase, RoundPhase::Finished) {
            physics.step(dt);
        }

        let ball = physics.ball_state();

        // Flight → AtRest 遷移時にカップ判定 + 水ペナルティ + 履歴記録。
        if shot.phase == ShotPhase::Flight {
            flight_timer += dt;
        }
        if round.phase == RoundPhase::Playing
            && shot.phase == ShotPhase::Flight
            && ball.at_rest
            && flight_timer > FLIGHT_REST_HOLD_SEC
        {
            let pin = course.pin_world_pos();
            let holed = check_hole_out(&ball, pin);
            let tile = course.tile_at(ball.pos[0].floor() as i32, ball.pos[1].floor() as i32);
            let penalty = !holed && tile == Some(TILE_WATER);
            round.record_stroke(
                shot.club,
                shot.power,
                round.last_start_pos,
                ball.pos,
                tile,
                penalty,
            );
            if holed {
                round.finish();
            } else if penalty {
                physics.teleport_ball(round.last_start_pos);
            }
            shot.notify_rest();
            flight_timer = 0.0;
        }

        // --- カメラ更新 ---
        follow.yaw = shot.yaw;
        let (ccx, ccy, ccz, cyaw, cpitch) = follow.update(ball.pos);
        cam.set_pose(ccx, ccy, cyaw);
        cam.set_z(ccz);
        cam.set_pitch(cpitch);

        // --- 描画 ---
        fb.clear(Color::default());
        let rays = cam.cast_all_rays(&course, fb_w, MAX_DISTANCE);
        render_floor_ceiling(&mut fb, &rays, &scene, &course, &cam, MAX_DISTANCE);
        render_walls(&mut fb, &rays, &scene, &course, &cam, MAX_DISTANCE);
        // ホールアウト後はボールを消す（カップに落ちた演出）。
        let mut sprites: Vec<Sprite> = Vec::with_capacity(2);
        sprites.push(Sprite {
            x: pin_x,
            y: pin_y,
            sprite_type: PIN_SPRITE_TYPE,
        });
        if !round.holed_out {
            sprites.push(Sprite {
                x: ball.pos[0],
                y: ball.pos[1],
                sprite_type: BALL_SPRITE_TYPE,
            });
        }
        let projected = project_sprites(&sprites, &cam, &course, fb_w, fb_h);
        render_sprites(&mut fb, &projected, &rays, &art, MAX_DISTANCE);

        // Finished フェーズはダイアログ背景として中央を暗くする。
        if round.phase == RoundPhase::Finished {
            dim_center(&mut fb);
        }

        // --- HUD / ステータス ---
        let status = match round.phase {
            RoundPhase::ParSelect => par_select_hud(fb_w),
            RoundPhase::Playing => play_hud(&shot, &round, ball.pos, ball.vel, &course, fb_w),
            RoundPhase::Finished => {
                // result は finish() で必ず埋まっている。
                let result = round.result.as_ref().expect("result after finish");
                end_hud(result, &round.stroke_log, fb_w)
            }
        };

        present(&fb, &status)?;
        std::thread::sleep(FRAME_SLEEP);
    }
}
