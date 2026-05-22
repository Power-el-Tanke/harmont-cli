//! Mission Control TUI — host-side ratatui renderer for `hm run`,
//! `hm dev up`, and `hm cloud build watch`. See
//! `docs/superpowers/specs/2026-05-22-tui-mission-control-design.md`.

// Submodules added in later tasks:
pub mod event;
// pub mod app;
// pub mod source;
// pub mod term;
// pub mod theme;
// pub mod fx;
// pub mod widgets;

#[derive(Debug, Clone)]
pub struct TuiOptions {
    pub fx_enabled: bool,
    pub summary_card: bool,
    pub title: String,
}

#[derive(Debug, thiserror::Error)]
pub enum TuiError {
    #[error("terminal i/o: {0}")]
    Io(#[from] std::io::Error),
    #[error("event channel closed before BuildEnd")]
    ChannelClosed,
}
