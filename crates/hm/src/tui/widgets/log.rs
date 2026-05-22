//! Log tail for the focused chain's most-recent step.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Widget};

use crate::tui::app::AppState;
use crate::tui::theme::Theme;

pub struct LogPane<'a> {
    pub state: &'a AppState,
    pub theme: &'a Theme,
    pub scroll: usize,
    pub filter: Option<&'a str>,
}

impl std::fmt::Debug for LogPane<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LogPane")
            .field("scroll", &self.scroll)
            .field("filter", &self.filter)
            .finish()
    }
}

impl Widget for LogPane<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let chain_label = self
            .state
            .chains
            .get(self.state.focused_chain)
            .map(|c| c.label.clone())
            .unwrap_or_default();
        let title = format!(" log · {chain_label} ");
        let block = Block::default()
            .borders(Borders::ALL)
            .title(title)
            .border_style(self.theme.border(true));
        let inner = block.inner(area);
        block.render(area, buf);

        let Some(step_id) = self.state.focused_step_id() else { return };
        let Some(log) = self.state.logs.get(&step_id) else { return };

        let entries: Vec<_> = log
            .entries
            .iter()
            .filter(|e| self.filter.is_none_or(|f| e.line.contains(f)))
            .collect();

        let height = inner.height as usize;
        let start = entries.len().saturating_sub(height + self.scroll);
        let visible: Vec<Line<'_>> = entries
            .iter()
            .skip(start)
            .take(height)
            .map(|entry| {
                let prefix = match entry.stream {
                    hm_plugin_protocol::StdStream::Stdout => "  ",
                    hm_plugin_protocol::StdStream::Stderr => "! ",
                };
                let style = if entry.stream == hm_plugin_protocol::StdStream::Stderr {
                    Style::default().fg(self.theme.text_dim)
                } else {
                    Style::default()
                };
                Line::from(vec![
                    Span::styled(prefix.to_string(), style),
                    Span::styled(entry.line.clone(), style),
                ])
            })
            .collect();

        Paragraph::new(visible).render(inner, buf);

        if log.dropped > 0 {
            let drop_msg = format!("  … {} events dropped (lagged) …", log.dropped);
            let style = Style::default().fg(self.theme.text_dim);
            let line = Line::styled(drop_msg, style);
            let drop_area = Rect::new(inner.x, inner.y, inner.width, 1);
            Paragraph::new(line).render(drop_area, buf);
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::tui::event::TuiEvent;
    use crate::tui::widgets::buffer_to_string;
    use uuid::Uuid;

    #[test]
    fn snapshot_log_with_filter() {
        let mut s = AppState::new();
        s.apply(TuiEvent::ChainQueued {
            chain_idx: 0,
            label: "c0".into(),
            parent: None,
        });
        let sid = Uuid::new_v4();
        s.apply(TuiEvent::StepStart {
            step_id: sid,
            chain_idx: 0,
            runner: "docker".into(),
            image: None,
            label: "test".into(),
        });
        for l in ["alpha", "beta cat", "gamma cat", "delta"] {
            s.apply(TuiEvent::StepLog {
                step_id: sid,
                stream: hm_plugin_protocol::StdStream::Stdout,
                line: l.into(),
                ts: chrono::Utc::now(),
            });
        }
        let theme = Theme::dark();
        let area = Rect::new(0, 0, 40, 6);
        let mut buf = Buffer::empty(area);
        LogPane { state: &s, theme: &theme, scroll: 0, filter: Some("cat") }
            .render(area, &mut buf);
        insta::assert_snapshot!(buffer_to_string(&buf));
    }
}
