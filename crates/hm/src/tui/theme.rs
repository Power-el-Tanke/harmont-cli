//! Single-theme palette. See spec §3.3.

use ratatui::style::{Color, Modifier, Style};

#[derive(Debug, Clone, Copy)]
pub struct Theme {
    pub border_dim: Color,
    pub border_focus: Color,
    pub accent_a: Color,
    pub accent_b: Color,
    pub pass: Color,
    pub cache: Color,
    pub fail: Color,
    pub running: Color,
    pub pending: Color,
    pub text_dim: Color,
}

impl Theme {
    #[must_use]
    pub const fn dark() -> Self {
        Self {
            border_dim: Color::DarkGray,
            border_focus: Color::Cyan,
            accent_a: Color::Cyan,
            accent_b: Color::Blue,
            pass: Color::Green,
            cache: Color::Yellow,
            fail: Color::Red,
            running: Color::Cyan,
            pending: Color::DarkGray,
            text_dim: Color::DarkGray,
        }
    }

    #[must_use]
    pub const fn border(&self, focused: bool) -> Style {
        let color = if focused { self.border_focus } else { self.border_dim };
        // Style::default() isn't const in older ratatui; use Style::new() which is.
        Style::new().fg(color)
    }

    #[must_use]
    pub fn status(&self, status: crate::tui::app::StepStatus) -> Style {
        use crate::tui::app::StepStatus;
        let c = match status {
            StepStatus::Queued => self.pending,
            StepStatus::Running => self.running,
            StepStatus::CachedHit => self.cache,
            StepStatus::Passed => self.pass,
            StepStatus::Failed => self.fail,
        };
        Style::new().fg(c).add_modifier(Modifier::BOLD)
    }
}
