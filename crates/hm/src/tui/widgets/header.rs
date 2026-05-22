//! Header widget — wordmark + run id + chain counter.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::Widget;

use crate::tui::app::{AppState, StepStatus};
use crate::tui::theme::Theme;

pub struct Header<'a> {
    pub state: &'a AppState,
    pub theme: &'a Theme,
    pub title: &'a str,
}

impl std::fmt::Debug for Header<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Header").field("title", &self.title).finish()
    }
}

impl Widget for Header<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let total_steps = self.state.steps.len();
        let done = self.state.steps.values()
            .filter(|s| matches!(s.status, StepStatus::Passed | StepStatus::CachedHit | StepStatus::Failed))
            .count();
        let chains = self.state.chains.len();
        let run_short = self.state.run_id
            .map_or_else(|| "—".into(), |u| format!("{:.8}", u.simple()));
        let title_text = format!(
            " HARMONT   {}   run {}   ·   {chains} chains · {done}/{total_steps} done ",
            self.title, run_short,
        );
        let style = Style::default()
            .fg(self.theme.accent_a)
            .add_modifier(Modifier::BOLD);
        let line = Line::styled(title_text, style);
        buf.set_line(area.x, area.y, &line, area.width);
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::tui::event::TuiEvent;
    use crate::tui::widgets::buffer_to_string;
    use hm_plugin_protocol::PlanSummary;
    use uuid::Uuid;

    fn fixture() -> AppState {
        let mut s = AppState::new();
        s.apply(TuiEvent::BuildStart {
            run_id: Uuid::nil(),
            plan: PlanSummary {
                step_count: 9,
                chain_count: 3,
                default_runner: "docker".into(),
            },
            started_at: chrono::Utc::now(),
        });
        for i in 0..3 {
            s.apply(TuiEvent::ChainQueued {
                chain_idx: i,
                label: format!("c{i}"),
                parent: None,
            });
        }
        s
    }

    #[test]
    fn snapshot_header_idle() {
        let theme = Theme::dark();
        let state = fixture();
        let mut buf = Buffer::empty(Rect::new(0, 0, 80, 1));
        Header { state: &state, theme: &theme, title: "hm run" }
            .render(Rect::new(0, 0, 80, 1), &mut buf);
        insta::assert_snapshot!(buffer_to_string(&buf));
    }
}
