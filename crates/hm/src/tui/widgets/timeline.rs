//! Gantt-style timeline. Bars per chain, colored by current step
//! status, with right-aligned label + duration + status pill.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Widget};

use crate::tui::app::{AppState, StepStatus};
use crate::tui::theme::Theme;

pub struct Timeline<'a> {
    pub state: &'a AppState,
    pub theme: &'a Theme,
}

impl std::fmt::Debug for Timeline<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Timeline").finish()
    }
}

const fn pill(status: StepStatus) -> &'static str {
    match status {
        StepStatus::Queued => "queued",
        StepStatus::Running => "run",
        StepStatus::CachedHit => "cache",
        StepStatus::Passed => "pass",
        StepStatus::Failed => "fail",
    }
}

impl Widget for Timeline<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .borders(Borders::ALL)
            .title(" timeline ")
            .border_style(self.theme.border(false));
        let inner = block.inner(area);
        block.render(area, buf);

        let total_ms: u64 = self.state.steps.values()
            .filter_map(|s| s.duration_ms)
            .sum::<u64>()
            .max(1);
        let bar_max = u64::from(inner.width.saturating_sub(28));
        let bar_max_u16 = u16::try_from(bar_max).unwrap_or(u16::MAX);

        let rows: Vec<Line<'_>> = self
            .state
            .chains
            .iter()
            .enumerate()
            .take(inner.height as usize)
            .filter_map(|(row, chain)| {
                let last_step_id = chain.steps.last()?;
                let step = self.state.steps.get(last_step_id)?;
                let dur = step.duration_ms.unwrap_or(0);

                #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss, clippy::cast_sign_loss)]
                let fill = ((dur as f64 / total_ms as f64) * bar_max as f64) as u16;

                let status_style = self.theme.status(step.status);
                let label = format!("c{} ", row + 1);
                let filled: String = "█".repeat(fill as usize);
                let pending_len = bar_max_u16.saturating_sub(fill) as usize;
                let pending: String = "░".repeat(pending_len);
                let trail = format!(" {} {dur:>4}ms {:>5}", step.label, pill(step.status));

                Some(Line::from(vec![
                    Span::raw(label),
                    Span::styled(filled, status_style),
                    Span::styled(pending, Style::default().fg(self.theme.pending)),
                    Span::raw(trail),
                ]))
            })
            .collect();

        Paragraph::new(rows).render(inner, buf);
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

    #[test]
    fn snapshot_timeline_three_chains() {
        let mut s = AppState::new();
        s.apply(TuiEvent::BuildStart {
            run_id: Uuid::nil(),
            plan: PlanSummary {
                step_count: 3,
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
            let sid = Uuid::new_v4();
            s.apply(TuiEvent::StepStart {
                step_id: sid,
                chain_idx: i,
                runner: "docker".into(),
                image: None,
                label: ["test", "build", "lint"][i].into(),
            });
            s.apply(TuiEvent::StepEnd {
                step_id: sid,
                exit_code: 0,
                duration_ms: (i as u64 + 1) * 1000,
            });
        }
        let theme = Theme::dark();
        let area = Rect::new(0, 0, 60, 8);
        let mut buf = Buffer::empty(area);
        Timeline { state: &s, theme: &theme }.render(area, &mut buf);
        insta::assert_snapshot!(buffer_to_string(&buf));
    }
}
