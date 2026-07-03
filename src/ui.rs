use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph};
use ratatui::Frame;

use crate::app::{App, OutputMode};
use crate::glyph::Glyph;
use crate::matcher::Hit;

pub fn render(frame: &mut Frame, app: &App, glyphs: &[Glyph]) {
    let area = frame.area();
    let [header, body, footer_rect] = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(area)[..]
    else {
        return;
    };

    let prompt = Paragraph::new(format!("> {}", app.query))
        .block(Block::default().borders(Borders::ALL).title("search"));
    frame.render_widget(prompt, header);

    let [list_area, preview_area] = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
        .split(body)[..]
    else {
        return;
    };

    render_list(frame, list_area, app, glyphs);
    render_preview(frame, preview_area, app, glyphs);
    render_footer(frame, footer_rect, app);
}

fn render_list(frame: &mut Frame, area: Rect, app: &App, glyphs: &[Glyph]) {
    let inner_h = area.height.saturating_sub(2) as usize;
    let start = app.scroll.min(app.items.len());
    let end = (start + inner_h).min(app.items.len());

    let items: Vec<ListItem> = app.items[start..end]
        .iter()
        .map(|h| ListItem::new(row_line(h, glyphs)))
        .collect();

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!("glyphs ({})", app.items.len())),
        )
        .highlight_style(
            Style::default()
                .add_modifier(Modifier::REVERSED)
                .fg(Color::Cyan),
        )
        .highlight_symbol("> ");

    let mut state = ListState::default();
    if app.cursor >= start && app.cursor < end {
        state.select(Some(app.cursor - start));
    }
    frame.render_stateful_widget(list, area, &mut state);
}

fn render_preview(frame: &mut Frame, area: Rect, app: &App, glyphs: &[Glyph]) {
    let block = Block::default().borders(Borders::ALL).title("preview");
    frame.render_widget(block, area);
    let inner = area.inner(ratatui::layout::Margin::new(1, 1));
    let width = inner.width as usize;

    let lines: Vec<Line> = match app.items.get(app.cursor) {
        Some(hit) => {
            let g = &glyphs[hit.glyph_idx];
            let display = g
                .char()
                .map(|c| c.to_string())
                .unwrap_or_else(|| "<none>".into());
            let esc = g
                .char()
                .map(|c| format!("\\u{{{:x}}}", c as u32))
                .unwrap_or_default();
            vec![
                Line::from(Span::styled(display, Style::default().fg(Color::Yellow)))
                    .alignment(Alignment::Center),
                Line::default().alignment(Alignment::Center),
                Line::from(name_spans(&g.name, &hit.name_indices)),
                Line::from(format!(
                    "set:  {} ({})",
                    g.set(),
                    aliases_summary(&g.aliases(), width)
                )),
                Line::from(format!("hex:  0x{:04x}", g.codepoint)),
                Line::from(format!("esc:  {esc}")),
            ]
        }
        None => vec![Line::from("no match").alignment(Alignment::Center)],
    };

    frame.render_widget(Paragraph::new(Text::from(lines)), inner);
}

fn render_footer(frame: &mut Frame, area: Rect, app: &App) {
    let total = app.items.len();
    let pos = if total == 0 { 0 } else { app.cursor + 1 };
    let count = format!(" {pos:>4}/{total:<4}");
    let hints = " \u{2191}/\u{2193} move  \u{21B5} select  esc/^C cancel  ^Y mode  set>query ";
    let badge = format!("OUTPUT: {} \u{2192} ", app.output_mode.label());
    let line = Line::from(vec![
        Span::styled(count, Style::default().add_modifier(Modifier::DIM)),
        Span::raw(hints),
        Span::styled(
            badge,
            Style::default()
                .fg(badge_color(app.output_mode))
                .add_modifier(Modifier::BOLD),
        ),
    ]);
    frame.render_widget(Paragraph::new(line), area);
}

/// A one-line list row for `hit`: glyph char + highlighted name.
fn row_line(hit: &Hit, glyphs: &[Glyph]) -> Line<'static> {
    let g = &glyphs[hit.glyph_idx];
    let head = g
        .char()
        .map(|c| format!("{c} "))
        .unwrap_or_else(|| "  ".into());
    let mut spans = name_spans(&g.name, &hit.name_indices);
    spans.insert(0, Span::raw(head));
    Line::from(spans)
}

fn badge_color(mode: OutputMode) -> Color {
    match mode {
        OutputMode::Char => Color::Green,
        OutputMode::Escape => Color::Magenta,
        OutputMode::Name => Color::Blue,
        OutputMode::Hex => Color::Yellow,
    }
}

/// Paint `name` with matched characters yellow+bold; everything else default.
/// Owned-string spans only — kept simple at the cost of one alloc per frame.
fn name_spans(name: &str, indices: &[u32]) -> Vec<Span<'static>> {
    let mut highlights = vec![false; name.chars().count()];
    for &i in indices {
        if let Some(slot) = highlights.get_mut(i as usize) {
            *slot = true;
        }
    }
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut buf = String::new();
    let mut cur_active: Option<bool> = None;
    for (i, c) in name.chars().enumerate() {
        let active = highlights[i];
        match cur_active {
            Some(a) if a == active => buf.push(c),
            Some(_) => {
                spans.push(Span::styled(std::mem::take(&mut buf), split_style(cur_active.unwrap())));
                cur_active = Some(active);
                buf.push(c);
            }
            None => {
                cur_active = Some(active);
                buf.push(c);
            }
        }
    }
    if let Some(a) = cur_active {
        spans.push(Span::styled(std::mem::take(&mut buf), split_style(a)));
    }
    spans
}

fn split_style(active: bool) -> Style {
    if active {
        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    }
}

/// Truncate the alias list to fit a panel width, with a trailing ellipsis.
fn aliases_summary(aliases: &[String], width: usize) -> String {
    if aliases.is_empty() {
        return "-".into();
    }
    let mut out = aliases.join(", ");
    if out.len() > width {
        let keep = width.saturating_sub(1).min(out.len());
        out.truncate(keep);
        out.push('\u{2026}');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::glyph::Glyph;
    use crate::matcher::Hit;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

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

    fn render_to_string(g: &[Glyph], app: &App) -> String {
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        terminal.draw(|f| render(f, app, g)).unwrap();
        let buf = terminal.backend().buffer();
        let mut out = String::new();
        for y in 0..24u16 {
            for x in 0..80u16 {
                out.push_str(buf.cell((x, y)).unwrap().symbol());
            }
            out.push('\n');
        }
        out
    }

    fn footer_line(g: &[Glyph], app: &App) -> String {
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        terminal.draw(|f| render(f, app, g)).unwrap();
        let buf = terminal.backend().buffer();
        let mut line = String::new();
        for x in 0..80u16 {
            line.push_str(buf.cell((x, 23)).unwrap().symbol());
        }
        line
    }

    fn press_char(app: &mut App, c: char) {
        app.update(
            crossterm::event::KeyEvent::new(
                crossterm::event::KeyCode::Char(c),
                crossterm::event::KeyModifiers::NONE,
            ),
            24,
        );
    }

    #[test]
    fn renders_selected_name_and_labels() {
        let g = fx();
        let app = App::new(&g);
        let rendered = render_to_string(&g, &app);
        assert!(rendered.contains("nf-cod-folder"), "{rendered}");
        assert!(rendered.contains("search"), "{rendered}");
        assert!(rendered.contains("preview"), "{rendered}");
        assert!(rendered.contains("set:  cod ("), "{rendered}");
    }

    #[test]
    fn footer_renders_mode_indicator_and_hints() {
        let g = fx();
        let mut app = App::new(&g);
        app.update(
            crossterm::event::KeyEvent::new(
                crossterm::event::KeyCode::Char('y'),
                crossterm::event::KeyModifiers::CONTROL,
            ),
            24,
        );
        let line = footer_line(&g, &app);
        assert!(line.contains("esc"), "footer was: {line}");
        assert!(line.contains("\u{2191}"), "footer was: {line}");
        assert!(line.contains("OUTPUT:"), "footer was: {line}");
    }

    #[test]
    fn renders_no_match() {
        let g = fx();
        let mut app = App::new(&g);
        for c in ['z', 'z', 'z'] {
            press_char(&mut app, c);
        }
        assert!(app.items.is_empty());
        let s = render_to_string(&g, &app);
        assert!(s.contains("0/") || s.contains("matches"), "{s}");
    }

    #[test]
    fn highlight_indices_populated_for_name_match() {
        let g = fx();
        let mut app = App::new(&g);
        for c in "folder".chars() {
            press_char(&mut app, c);
        }
        let hit = app
            .items
            .iter()
            .find(|h: &&Hit| h.glyph_idx == 0)
            .expect("folder must hit cod");
        assert!(!hit.name_indices.is_empty());
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        terminal.draw(|f| render(f, &app, &g)).unwrap();
    }

    #[test]
    fn name_spans_includes_all_chars_when_all_match() {
        let spans = name_spans("abcdef", &[0, 1, 2, 3, 4, 5]);
        let joined: String = spans.iter().map(|s| s.content.clone()).collect();
        assert_eq!(joined, "abcdef");
        assert_eq!(spans.len(), 1);
    }

    #[test]
    fn name_spans_merges_consecutive_inactive_chars() {
        let spans = name_spans("aBcdD", &[0, 4]);
        let joined: String = spans.iter().map(|s| s.content.clone()).collect();
        assert_eq!(joined, "aBcdD");
        assert_eq!(spans.len(), 3);
        assert_eq!(spans[0].content, "a");
        assert_eq!(spans[1].content, "Bcd");
        assert_eq!(spans[2].content, "D");
    }

    #[test]
    fn name_spans_runs_for_all_inactive() {
        let spans = name_spans("abcdef", &[]);
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].content, "abcdef");
    }
}
