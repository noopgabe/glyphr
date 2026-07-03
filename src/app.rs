use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use nucleo_matcher::{Config, Matcher};

use crate::glyph::Glyph;
use crate::matcher::{self, Hit};

/// What `App::update` wants the run loop to do next.
#[derive(Debug)]
pub enum Action {
    Continue,
    Cancel,
    Quit(usize),
}

/// What glyphr emits to stdout and the clipboard when a glyph is selected.
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub enum OutputMode {
    /// The glyph character itself, e.g. ``.
    #[default]
    Char,
    /// A unicode escape usable in any string literal, e.g. `\u{f07d}`.
    Escape,
    /// The canonical nf- name, e.g. `nf-md-folder`.
    Name,
    /// Lowercase hex codepoint with `0x` prefix, e.g. `0xf0493`.
    Hex,
}

impl OutputMode {
    /// Cycle to the next mode in display order.
    pub fn next(self) -> Self {
        match self {
            Self::Char => Self::Escape,
            Self::Escape => Self::Name,
            Self::Name => Self::Hex,
            Self::Hex => Self::Char,
        }
    }

    /// Short status string shown in the footer.
    pub fn label(self) -> &'static str {
        match self {
            Self::Char => "char",
            Self::Escape => "esc",
            Self::Name => "name",
            Self::Hex => "hex",
        }
    }

    /// Format `g` into the string that should hit stdout and the clipboard
    /// when this mode is active.
    pub fn render(self, g: &Glyph) -> String {
        match self {
            Self::Char => g.char().map(|c| c.to_string()).unwrap_or_default(),
            Self::Escape => g
                .char()
                .map(|c| format!("\\u{{{:x}}}", c as u32))
                .unwrap_or_default(),
            Self::Name => g.name.clone(),
            Self::Hex => format!("0x{:04x}", g.codepoint),
        }
    }
}

pub struct App<'a> {
    glyphs: &'a [Glyph],
    matcher: Matcher,
    pub query: String,
    pub items: Vec<Hit>,
    pub cursor: usize,
    pub scroll: usize,
    pub output_mode: OutputMode,
}

impl<'a> App<'a> {
    pub fn new(glyphs: &'a [Glyph]) -> Self {
        let mut app = Self {
            glyphs,
            matcher: Matcher::new(Config::DEFAULT),
            query: String::new(),
            items: Vec::new(),
            cursor: 0,
            scroll: 0,
            output_mode: OutputMode::default(),
        };
        app.refilter();
        app
    }

    pub fn selected(&self) -> Option<usize> {
        self.items.get(self.cursor).map(|h| h.glyph_idx)
    }

    fn refilter(&mut self) {
        self.items = matcher::filter(self.glyphs, &self.query, &mut self.matcher);
        self.cursor = self.cursor.min(self.items.len().saturating_sub(1));
        self.scroll = 0;
    }

    fn list_height(height: usize) -> usize {
        // 3 (input) + 1 (footer) + 2 (list border) = 6 rows above the list inner.
        height.saturating_sub(6)
    }

    fn ensure_visible(&mut self, height: usize) {
        let lh = Self::list_height(height).max(1);
        let max = self.items.len();
        if max == 0 {
            self.scroll = 0;
            return;
        }
        if self.cursor >= max {
            self.cursor = max - 1;
        }
        if self.scroll > self.cursor {
            self.scroll = self.cursor;
        }
        if self.cursor >= self.scroll + lh {
            self.scroll = (self.cursor + 1).saturating_sub(lh);
        }
        if self.scroll + lh > max {
            self.scroll = max.saturating_sub(lh);
        }
    }

    pub fn update(&mut self, key: KeyEvent, height: usize) -> Action {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        if key.code == KeyCode::Esc || (key.code == KeyCode::Char('c') && ctrl) {
            return Action::Cancel;
        }
        match key.code {
            KeyCode::Enter => {
                return match self.selected() {
                    Some(idx) => Action::Quit(idx),
                    None => Action::Continue,
                };
            }
            KeyCode::Backspace => {
                self.query.pop();
                self.refilter();
                self.cursor = 0;
                self.ensure_visible(height);
            }
            KeyCode::Char(c) if !c.is_control() && !ctrl => {
                self.query.push(c);
                self.refilter();
                self.cursor = 0;
                self.ensure_visible(height);
            }
            KeyCode::Up | KeyCode::Char('k') if !ctrl => {
                self.cursor = self.cursor.saturating_sub(1);
                self.ensure_visible(height);
            }
            KeyCode::Down | KeyCode::Char('j') if !ctrl => {
                if self.cursor + 1 < self.items.len() {
                    self.cursor += 1;
                }
                self.ensure_visible(height);
            }
            KeyCode::PageUp => {
                let lh = Self::list_height(height).max(1);
                self.cursor = self.cursor.saturating_sub(lh);
                self.ensure_visible(height);
            }
            KeyCode::PageDown => {
                let lh = Self::list_height(height).max(1);
                self.cursor = (self.cursor + lh).min(self.items.len().saturating_sub(1));
                self.ensure_visible(height);
            }
            KeyCode::Home => {
                self.cursor = 0;
                self.scroll = 0;
            }
            KeyCode::End => {
                self.cursor = self.items.len().saturating_sub(1);
                self.ensure_visible(height);
            }
            KeyCode::Char('y') if ctrl => {
                self.output_mode = self.output_mode.next();
            }
            _ => {}
        }
        Action::Continue
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::glyph::Glyph;

    fn fx() -> Vec<Glyph> {
        vec![
            Glyph {
                name: "nf-cod-folder".into(),
                codepoint: 0xf07d,
            },
            Glyph {
                name: "nf-fa-cogs".into(),
                codepoint: 0xf085,
            },
            Glyph {
                name: "nf-dev-github".into(),
                codepoint: 0xe70e,
            },
            Glyph {
                name: "nf-md-cog".into(),
                codepoint: 0xf0493,
            },
        ]
    }

    fn key(code: KeyCode, mods: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, mods)
    }

    fn items_idx(app: &App<'_>) -> Vec<usize> {
        app.items.iter().map(|h| h.glyph_idx).collect()
    }

    #[test]
    fn enter_selects_highlighted() {
        let g = fx();
        let mut app = App::new(&g);
        app.update(key(KeyCode::Down, KeyModifiers::NONE), 24);
        app.update(key(KeyCode::Down, KeyModifiers::NONE), 24);
        let act = app.update(key(KeyCode::Enter, KeyModifiers::NONE), 24);
        match act {
            Action::Quit(idx) => assert_eq!(idx, app.items[2].glyph_idx),
            other => panic!("expected Quit, got {other:?}"),
        }
    }

    #[test]
    fn esc_cancels() {
        let g = fx();
        let mut app = App::new(&g);
        let act = app.update(key(KeyCode::Esc, KeyModifiers::NONE), 24);
        assert!(matches!(act, Action::Cancel));
    }

    #[test]
    fn ctrl_c_cancels() {
        let g = fx();
        let mut app = App::new(&g);
        let act = app.update(key(KeyCode::Char('c'), KeyModifiers::CONTROL), 24);
        assert!(matches!(act, Action::Cancel));
    }

    #[test]
    fn typing_filters_then_selects() {
        let g = fx();
        let mut app = App::new(&g);
        app.update(key(KeyCode::Char('f'), KeyModifiers::NONE), 24);
        app.update(key(KeyCode::Char('a'), KeyModifiers::NONE), 24);
        assert_eq!(items_idx(&app), vec![1]); // only nf-fa-cogs remains
        let act = app.update(key(KeyCode::Enter, KeyModifiers::NONE), 24);
        assert_eq!(g[1].codepoint, 0xf085);
        match act {
            Action::Quit(idx) => assert_eq!(idx, 1),
            other => panic!("expected Quit, got {other:?}"),
        }
    }

    #[test]
    fn backspace_unfilters() {
        let g = fx();
        let mut app = App::new(&g);
        // "co" matches: nf-cod-folder (name), nf-fa-cogs (alias "cogs"), nf-md-cog (name).
        app.update(key(KeyCode::Char('c'), KeyModifiers::NONE), 24);
        app.update(key(KeyCode::Char('o'), KeyModifiers::NONE), 24);
        assert_eq!(items_idx(&app).len(), 3);
        // Backspacing each char out restores the full list.
        app.update(key(KeyCode::Backspace, KeyModifiers::NONE), 24);
        app.update(key(KeyCode::Backspace, KeyModifiers::NONE), 24);
        assert_eq!(items_idx(&app).len(), g.len());
    }

    #[test]
    fn enter_with_no_match_continues() {
        let g = fx();
        let mut app = App::new(&g);
        app.update(key(KeyCode::Char('z'), KeyModifiers::NONE), 24);
        app.update(key(KeyCode::Char('z'), KeyModifiers::NONE), 24);
        assert!(app.items.is_empty());
        let act = app.update(key(KeyCode::Enter, KeyModifiers::NONE), 24);
        assert!(matches!(act, Action::Continue));
    }

    #[test]
    fn cursor_does_not_overshoot() {
        let g = fx();
        let mut app = App::new(&g);
        for _ in 0..20 {
            app.update(key(KeyCode::Down, KeyModifiers::NONE), 24);
        }
        assert_eq!(app.cursor, g.len() - 1);
    }

    #[test]
    fn set_prefix_typing_filters() {
        let g = fx();
        let mut app = App::new(&g);
        for c in "md>".chars() {
            app.update(key(KeyCode::Char(c), KeyModifiers::NONE), 24);
        }
        assert_eq!(items_idx(&app), vec![3]);
    }

    #[test]
    fn backspace_through_set_prefix() {
        let g = fx();
        let mut app = App::new(&g);
        for c in "md>".chars() {
            app.update(key(KeyCode::Char(c), KeyModifiers::NONE), 24);
        }
        assert_eq!(items_idx(&app), vec![3]);
        // Backspace to "md"; empty needle still respects the prefix.
        app.update(key(KeyCode::Backspace, KeyModifiers::NONE), 24);
        assert_eq!(app.query, "md");
        // Empty search with a prefix returns every prefixed glyph.
        assert!(items_idx(&app).contains(&3));
        // Backspace to "m"; fuzzy "m" still hits nf-md-cog ('m' at position 3).
        app.update(key(KeyCode::Backspace, KeyModifiers::NONE), 24);
        assert_eq!(app.query, "m");
        assert_eq!(items_idx(&app), vec![3]);
        // Backspace out the prefix entirely; full list back.
        app.update(key(KeyCode::Backspace, KeyModifiers::NONE), 24);
        assert_eq!(app.query, "");
        assert_eq!(items_idx(&app).len(), g.len());
    }

    #[test]
    fn ctrl_y_cycles_output_mode() {
        let g = fx();
        let mut app = App::new(&g);
        assert_eq!(app.output_mode, OutputMode::Char);
        app.update(key(KeyCode::Char('y'), KeyModifiers::CONTROL), 24);
        assert_eq!(app.output_mode, OutputMode::Escape);
        app.update(key(KeyCode::Char('y'), KeyModifiers::CONTROL), 24);
        assert_eq!(app.output_mode, OutputMode::Name);
        app.update(key(KeyCode::Char('y'), KeyModifiers::CONTROL), 24);
        assert_eq!(app.output_mode, OutputMode::Hex);
        app.update(key(KeyCode::Char('y'), KeyModifiers::CONTROL), 24);
        assert_eq!(app.output_mode, OutputMode::Char);
    }

    #[test]
    fn output_mode_next_wraps() {
        assert_eq!(OutputMode::Hex.next(), OutputMode::Char);
        assert_eq!(OutputMode::Char.next(), OutputMode::Escape);
    }
}
