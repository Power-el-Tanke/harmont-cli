//! `?` help overlay — centered card.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::widgets::{Block, Borders, Widget};

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

        let lines = [
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
        ];
        for (i, l) in lines.iter().enumerate() {
            let y = inner.y + 1 + u16::try_from(i).unwrap_or(u16::MAX);
            if y >= inner.y + inner.height { break; }
            let mut x = inner.x + 2;
            for ch in l.chars() {
                if x >= inner.x + inner.width { break; }
                if let Some(cell) = buf.cell_mut((x, y)) {
                    cell.set_symbol(&ch.to_string());
                }
                x += 1;
            }
        }
    }
}
