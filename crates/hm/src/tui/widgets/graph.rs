//! Chain DAG renderer. One row per chain; step glyphs grouped left to
//! right by chain order.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Widget};

use crate::tui::app::{AppState, StepStatus};
use crate::tui::theme::Theme;

pub struct Graph<'a> {
    pub state: &'a AppState,
    pub theme: &'a Theme,
}

impl std::fmt::Debug for Graph<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Graph").finish()
    }
}

const fn glyph(status: StepStatus) -> &'static str {
    match status {
        StepStatus::Queued => "●",
        StepStatus::Running => "◐",
        StepStatus::CachedHit => "◆",
        StepStatus::Passed => "◇",
        StepStatus::Failed => "✖",
    }
}

impl Widget for Graph<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .borders(Borders::ALL)
            .title(" graph ")
            .border_style(self.theme.border(false));
        let inner = block.inner(area);
        block.render(area, buf);

        let max_rows = inner.height as usize;
        let rows: Vec<Line<'_>> = self
            .state
            .chains
            .iter()
            .take(max_rows)
            .map(|chain| {
                let mut spans: Vec<Span<'_>> = Vec::new();
                let mut first = true;
                for sid in &chain.steps {
                    let Some(step) = self.state.steps.get(sid) else { continue };
                    if !first {
                        spans.push(Span::raw("─"));
                    }
                    spans.push(Span::styled(
                        glyph(step.status).to_string(),
                        self.theme.status(step.status),
                    ));
                    first = false;
                }
                Line::from(spans)
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
    fn snapshot_graph_three_chains_mixed_status() {
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
        let s0 = Uuid::new_v4();
        let s1 = Uuid::new_v4();
        let s2 = Uuid::new_v4();
        s.apply(TuiEvent::StepStart { step_id: s0, chain_idx: 0, runner: "docker".into(), image: None, label: "test".into() });
        s.apply(TuiEvent::StepEnd { step_id: s0, exit_code: 0, duration_ms: 100 });
        s.apply(TuiEvent::StepStart { step_id: s1, chain_idx: 1, runner: "docker".into(), image: None, label: "build".into() });
        s.apply(TuiEvent::StepCacheHit { step_id: s1, key: "k".into(), tag: "t".into() });
        s.apply(TuiEvent::StepStart { step_id: s2, chain_idx: 2, runner: "docker".into(), image: None, label: "lint".into() });

        let theme = Theme::dark();
        let area = Rect::new(0, 0, 30, 8);
        let mut buf = Buffer::empty(area);
        Graph { state: &s, theme: &theme }.render(area, &mut buf);
        insta::assert_snapshot!(buffer_to_string(&buf));
    }
}
