//! Final summary card — full-screen frame after `BuildEnd`.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Widget};
use tui_big_text::{BigText, PixelSize};

use crate::tui::app::{AppState, StepStatus};
use crate::tui::theme::Theme;

pub struct Summary<'a> {
    pub state: &'a AppState,
    pub theme: &'a Theme,
}

impl std::fmt::Debug for Summary<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Summary").finish()
    }
}

impl Widget for Summary<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(self.theme.border(true));
        let inner = block.inner(area);
        block.render(area, buf);

        let mut pass = 0_usize;
        let mut cache = 0_usize;
        let mut fail = 0_usize;
        let mut slowest: Option<(String, u64)> = None;
        for s in self.state.steps.values() {
            match s.status {
                StepStatus::Passed => pass += 1,
                StepStatus::CachedHit => cache += 1,
                StepStatus::Failed => fail += 1,
                _ => {}
            }
            if let Some(d) = s.duration_ms {
                if slowest.as_ref().map_or(true, |(_, p)| d > *p) {
                    slowest = Some((s.label.clone(), d));
                }
            }
        }
        let total = self.state.steps.len().max(1);
        #[allow(clippy::cast_precision_loss)]
        let cache_pct = (cache as f64 / total as f64) * 100.0;
        let total_ms: u64 = self.state.steps.values()
            .filter_map(|s| s.duration_ms)
            .sum();

        let failed = fail > 0;
        let banner_style = if failed {
            Style::default().fg(self.theme.fail).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(self.theme.pass).add_modifier(Modifier::BOLD)
        };
        let banner = if failed { "build failed" } else { "build complete" };

        // Big wordmark.
        let big = BigText::builder()
            .pixel_size(PixelSize::Quadrant)
            .style(Style::default().fg(self.theme.accent_a))
            .lines(vec![Line::raw("HARMONT")])
            .build();
        let wordmark_area = Rect::new(
            inner.x + 2,
            inner.y + 1,
            inner.width.saturating_sub(4),
            4,
        );
        big.render(wordmark_area, buf);

        let line_banner = banner.to_string();
        let line_total = format!("  total       {total_ms}ms");
        let line_chains = format!("  chains      {}", self.state.chains.len());
        let line_steps = format!("  steps       {pass} passed · {cache} cached · {fail} failed");
        let line_cache = format!("  cache hit % {cache_pct:.0}%");
        let line_slowest = format!(
            "  slowest     {}",
            slowest.as_ref().map_or_else(String::new, |(l, d)| format!("{l} ({d}ms)")),
        );

        let lines: [(&str, Style); 7] = [
            (&line_banner, banner_style),
            ("", Style::default()),
            (&line_total, Style::default()),
            (&line_chains, Style::default()),
            (&line_steps, Style::default()),
            (&line_cache, Style::default()),
            (&line_slowest, Style::default()),
        ];
        for (i, (text, style)) in lines.iter().enumerate() {
            let y = inner.y + 6 + u16::try_from(i).unwrap_or(u16::MAX);
            if y >= inner.y + inner.height { break; }
            let mut x = inner.x + 2;
            for ch in text.chars() {
                if x >= inner.x + inner.width { break; }
                if let Some(cell) = buf.cell_mut((x, y)) {
                    cell.set_symbol(&ch.to_string()).set_style(*style);
                }
                x += 1;
            }
        }
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
    fn snapshot_summary_pass() {
        let mut s = AppState::new();
        s.apply(TuiEvent::BuildStart {
            run_id: Uuid::nil(),
            plan: PlanSummary { step_count: 3, chain_count: 3, default_runner: "docker".into() },
            started_at: chrono::Utc::now(),
        });
        for i in 0..3 {
            s.apply(TuiEvent::ChainQueued { chain_idx: i, label: format!("c{i}"), parent: None });
            let sid = Uuid::new_v4();
            s.apply(TuiEvent::StepStart { step_id: sid, chain_idx: i, runner: "docker".into(), image: None, label: ["test", "build", "lint"][i].into() });
            s.apply(TuiEvent::StepEnd { step_id: sid, exit_code: 0, duration_ms: (i as u64 + 1) * 1000 });
        }
        s.apply(TuiEvent::BuildEnd { exit_code: 0, duration_ms: 6000 });

        let theme = Theme::dark();
        let area = Rect::new(0, 0, 80, 24);
        let mut buf = Buffer::empty(area);
        Summary { state: &s, theme: &theme }.render(area, &mut buf);
        insta::assert_snapshot!(buffer_to_string(&buf));
    }
}
