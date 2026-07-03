use std::io::{stdout, Write};

use anyhow::Result;
use clap::Parser;
use crossterm::event::{self, Event, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use glyphr::app::Action;
use glyphr::{app, glyph::Glyph, snapshot, ui};

#[derive(Parser)]
#[command(version, about = "Fuzzy Nerd Font glyph picker")]
struct Cli {}

struct Guard;
impl Drop for Guard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(stdout(), LeaveAlternateScreen);
    }
}

fn main() -> Result<()> {
    let _ = Cli::parse();
    let glyphs = snapshot::glyphs();

    enable_raw_mode()?;
    let mut out = stdout();
    execute!(out, EnterAlternateScreen)?;
    let _guard = Guard;
    let backend = CrosstermBackend::new(out);
    let mut terminal = Terminal::new(backend)?;

    let outcome = run(&mut terminal, glyphs);
    drop(_guard);

    match outcome {
        Ok(Some(payload)) => {
            // Always stamp to stdout so fzf-style paste flows work.
            print!("{payload}");
            std::io::stdout().flush()?;
            // Mirror onto the clipboard for convenience.
            //
            // On Wayland, `wl-copy` forks a daemon that survives glyphr's
            // exit so the selection stays alive (Wayland selections are
            // pull-based — the source process must keep serving paste
            // requests). On X11, and as a Wayland fallback when wl-copy is
            // unavailable, use arboard. arboard warns if `Clipboard` is
            // dropped within ~100ms of `set_text` (debug builds); keep it
            // briefly alive so clipboard managers have a chance to observe
            // the contents.
            let wlcopy_ok = std::env::var_os("WAYLAND_DISPLAY").is_some()
                && std::process::Command::new("wl-copy")
                    .arg(&payload)
                    .status()
                    .map(|s| s.success())
                    .unwrap_or(false);
            if !wlcopy_ok {
                if let Ok(mut cb) = arboard::Clipboard::new() {
                    let _ = cb.set_text(payload);
                    std::thread::sleep(std::time::Duration::from_millis(250));
                }
            }
            Ok(())
        }
        Ok(None) => Ok(()),
        Err(e) => {
            eprintln!("glyphr: {e:?}");
            Err(e)
        }
    }
}

fn run(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    glyphs: &[Glyph],
) -> Result<Option<String>> {
    let mut app = app::App::new(glyphs);
    loop {
        terminal.draw(|f| ui::render(f, &app, glyphs))?;
        if let Event::Key(key) = event::read()? {
            if key.kind == KeyEventKind::Press {
                let h = terminal.size()?.height as usize;
                match app.update(key, h) {
                    Action::Continue => {}
                    Action::Cancel => return Ok(None),
                    Action::Quit(idx) => return Ok(Some(app.output_mode.render(&glyphs[idx]))),
                }
            }
        }
    }
}
