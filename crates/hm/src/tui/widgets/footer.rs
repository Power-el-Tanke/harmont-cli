//! Footer — keybinding hints + summary counters.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::widgets::Widget;

use crate::tui::app::{AppState, StepStatus};
use crate::tui::theme::Theme;

pub struct Footer<'a> {
    pub state: &'a AppState,
    pub theme: &'a Theme,
}

impl std::fmt::Debug for Footer<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Footer").finish()
    }
}

impl Widget for Footer<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let mut pass = 0_usize;
        let mut cache = 0_usize;
        let mut fail = 0_usize;
        for s in self.state.steps.values() {
            match s.status {
                StepStatus::Passed => pass += 1,
                StepStatus::CachedHit => cache += 1,
                StepStatus::Failed => fail += 1,
                _ => {}
            }
        }
        let hints = " [tab] chain · [l] logs · [/] filter · [q] quit ";
        let summary = format!(" {pass} pass · {cache} cache · {fail} fail ");
        let total_width = area.width as usize;
        let pad = total_width.saturating_sub(hints.len() + summary.len());
        let line = format!("{hints}{}{summary}", " ".repeat(pad));

        let mut x = area.x;
        for ch in line.chars() {
            if x >= area.x + area.width { break; }
            if let Some(cell) = buf.cell_mut((x, area.y)) {
                cell.set_symbol(&ch.to_string())
                    .set_style(ratatui::style::Style::default().fg(self.theme.text_dim));
            }
            x += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::widgets::buffer_to_string;

    #[test]
    fn snapshot_footer_empty() {
        let s = AppState::new();
        let theme = Theme::dark();
        let area = Rect::new(0, 0, 80, 1);
        let mut buf = Buffer::empty(area);
        Footer { state: &s, theme: &theme }.render(area, &mut buf);
        insta::assert_snapshot!(buffer_to_string(&buf));
    }
}
