//! street-golf — Phase 0 skeleton.
//!
//! Blanks the termray framebuffer with a meadow-green background, presents a
//! single frame via crossterm half-block rendering, pauses briefly, and exits.
//! Phase 1 will replace this with a synthetic course and a playable hole.

use std::io::{Write, stdout};

use crossterm::cursor::{Hide, Show};
use crossterm::style::{
    Color as CtColor, Print, ResetColor, SetBackgroundColor, SetForegroundColor,
};
use crossterm::terminal::{
    Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode,
    enable_raw_mode, size,
};
use crossterm::{execute, queue};
use termray::{Color, Framebuffer};

/// RAII guard that enters the alternate screen in raw mode on construction
/// and restores the terminal on drop. This ensures the terminal is cleaned up
/// even if the program panics or returns early.
struct TerminalGuard;

impl TerminalGuard {
    fn new() -> anyhow::Result<Self> {
        enable_raw_mode()?;
        execute!(stdout(), EnterAlternateScreen, Hide, Clear(ClearType::All))?;
        Ok(Self)
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        // Best-effort cleanup. Errors here can't propagate but also
        // can't be meaningfully recovered from — we've already lost
        // control of the terminal either way.
        let _ = execute!(stdout(), Show, LeaveAlternateScreen);
        let _ = disable_raw_mode();
    }
}

fn main() -> anyhow::Result<()> {
    let (cols, rows) = size()?;
    let fb_w = cols as usize;
    // Reserve two rows for the terminal prompt, render at 2x vertical resolution
    // via half-block characters (one cell = top + bottom pixel).
    let fb_h = (rows as usize).saturating_sub(2) * 2;
    if fb_w == 0 || fb_h == 0 {
        // Too tiny to draw anything meaningful. Exit cleanly.
        return Ok(());
    }

    let mut fb = Framebuffer::new(fb_w, fb_h);
    fb.clear(Color::rgb(20, 40, 25));

    let _guard = TerminalGuard::new()?;

    render_frame(&fb)?;

    // Phase 0 scaffolding: fixed display time so the blank frame is visible
    // for a moment before exit. Replace with the real shot loop in Phase 3 (#4).
    std::thread::sleep(std::time::Duration::from_millis(800));

    Ok(())
}

fn render_frame(fb: &Framebuffer) -> anyhow::Result<()> {
    let mut out = stdout();
    let height = fb.height();
    if height == 0 {
        return Ok(());
    }
    for y in (0..height).step_by(2) {
        for x in 0..fb.width() {
            let top = fb.get_pixel(x, y);
            let bot = fb.get_pixel(x, (y + 1).min(height - 1));
            queue!(
                out,
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
        queue!(out, ResetColor, Print("\r\n"))?;
    }
    out.flush()?;
    Ok(())
}
