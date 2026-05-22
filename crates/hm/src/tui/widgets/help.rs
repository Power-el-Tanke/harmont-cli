//! `?` help overlay — centered card.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Paragraph, Widget};

use crate::tui::theme::Theme;

pub struct Help<'a> { pub theme: &'a Theme }

impl std::fmt::Debug for Help<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Help").finish()
    }
}

impl Widget for Help<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .borders(Borders::ALL)
            .title(" help ")
            .border_style(self.theme.border(true));
        let inner = block.inner(area);
        block.render(area, buf);

        let lines: Vec<Line<'_>> = [
            "  q · Esc      quit",
            "  Tab          next chain",
            "  Shift-Tab    prev chain",
            "  l            expand log pane",
            "  / · Esc      filter logs",
            "  ↑ ↓ wheel    scroll log",
            "  PgUp PgDn    page-scroll log",
            "  g · G        top / bottom of log",
            "  ?            toggle this help",
            "  Ctrl-C       cancel run (twice to force)",
        ]
        .into_iter()
        .map(Line::raw)
        .collect();
        let body_area = Rect::new(
            inner.x + 2,
            inner.y + 1,
            inner.width.saturating_sub(2),
            inner.height.saturating_sub(1),
        );
        Paragraph::new(lines).render(body_area, buf);
    }
}
