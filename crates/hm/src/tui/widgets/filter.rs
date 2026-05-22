//! Inline filter prompt — single line at the bottom of the log pane.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::widgets::{Paragraph, Widget};

use crate::tui::theme::Theme;

pub struct Filter<'a> {
    pub theme: &'a Theme,
    pub query: &'a str,
}

impl std::fmt::Debug for Filter<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Filter").field("query", &self.query).finish()
    }
}

impl Widget for Filter<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let prompt = format!(" /{}_", self.query);
        let line = Line::styled(prompt, Style::default().fg(self.theme.accent_a));
        Paragraph::new(line).render(area, buf);
    }
}
