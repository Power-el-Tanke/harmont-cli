//! Mission Control TUI — host-side ratatui renderer.

pub mod app;
pub mod event;
pub mod fx;
pub mod source;
pub mod term;
pub mod theme;
pub mod widgets;

use std::io;
use std::time::{Duration, Instant};

use crossterm::event::{
    self as ce, Event as CeEvent, KeyCode, KeyEventKind, KeyModifiers, MouseEventKind,
};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use tokio::sync::mpsc;

use self::app::AppState;
use self::event::TuiEvent;
use self::fx::FxQueue;
use self::term::TermGuard;
use self::theme::Theme;
use self::widgets::filter::Filter;
use self::widgets::footer::Footer;
use self::widgets::graph::Graph;
use self::widgets::header::Header;
use self::widgets::help::Help;
use self::widgets::log::LogPane;
use self::widgets::summary::Summary;
use self::widgets::timeline::Timeline;

#[derive(Debug, Clone)]
pub struct TuiOptions {
    pub fx_enabled: bool,
    pub summary_card: bool,
    pub title: String,
}

#[derive(Debug, thiserror::Error)]
pub enum TuiError {
    #[error("terminal i/o: {0}")]
    Io(#[from] io::Error),
    #[error("event channel closed before BuildEnd")]
    ChannelClosed,
}

const FRAME_INTERVAL: Duration = Duration::from_millis(16);
const SUMMARY_HOLD: Duration = Duration::from_secs(2);
const MIN_COLS: u16 = 60;
const MIN_ROWS: u16 = 20;

/// Drive the Mission Control TUI. Consumes `TuiEvent`s from `events`
/// and renders until `BuildEnd` (or the user presses `q`/`Esc`/2×Ctrl-C).
///
/// # Errors
/// Returns `TuiError::Io` for terminal-setup or draw failures.
pub async fn run(
    mut events: mpsc::Receiver<TuiEvent>,
    opts: TuiOptions,
) -> Result<i32, TuiError> {
    let mut guard = TermGuard::enter()?;
    let theme = Theme::dark();
    let mut state = AppState::new();
    let mut fx = FxQueue::new(opts.fx_enabled);

    let mut frame_tick = tokio::time::interval(FRAME_INTERVAL);
    frame_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    let mut last_frame = Instant::now();
    let mut needs_render = true;
    let mut help_open = false;
    let mut filter_open = false;
    let mut filter_buf = String::new();
    let mut log_scroll: usize = 0;
    let mut last_ctrl_c: Option<Instant> = None;

    loop {
        tokio::select! {
            _ = frame_tick.tick() => {
                let now = Instant::now();
                let delta = now - last_frame;
                last_frame = now;

                // Drain pending key/mouse events (non-blocking).
                while ce::poll(Duration::from_millis(0)).map_err(TuiError::Io)? {
                    let ev = ce::read().map_err(TuiError::Io)?;
                    needs_render = true;
                    match ev {
                        CeEvent::Key(k) if k.kind == KeyEventKind::Press => {
                            if filter_open {
                                match k.code {
                                    KeyCode::Esc => { filter_open = false; filter_buf.clear(); }
                                    KeyCode::Backspace => { filter_buf.pop(); }
                                    KeyCode::Enter => { filter_open = false; }
                                    KeyCode::Char(c) => { filter_buf.push(c); }
                                    _ => {}
                                }
                                continue;
                            }
                            match k.code {
                                KeyCode::Char('q') | KeyCode::Esc => {
                                    return finalise(&state, opts.summary_card, &theme, &mut guard).await;
                                }
                                KeyCode::Tab => state.cycle_focus(1),
                                KeyCode::BackTab => state.cycle_focus(-1),
                                KeyCode::Char('/') => { filter_open = true; filter_buf.clear(); }
                                KeyCode::Char('?') => { help_open = !help_open; }
                                KeyCode::Up => { log_scroll = log_scroll.saturating_add(1); }
                                KeyCode::Down => { log_scroll = log_scroll.saturating_sub(1); }
                                KeyCode::PageUp => { log_scroll = log_scroll.saturating_add(10); }
                                KeyCode::PageDown => { log_scroll = log_scroll.saturating_sub(10); }
                                KeyCode::Char('g') => { log_scroll = usize::MAX / 2; }
                                KeyCode::Char('G') => { log_scroll = 0; }
                                KeyCode::Char('c') if k.modifiers.contains(KeyModifiers::CONTROL) => {
                                    let now = Instant::now();
                                    if last_ctrl_c.is_some_and(|t| now - t < Duration::from_secs(2)) {
                                        return Ok(130);
                                    }
                                    last_ctrl_c = Some(now);
                                    // First Ctrl-C: orchestrator cancellation is hooked
                                    // separately via signal::install_ctrlc; this branch
                                    // exists so a second Ctrl-C within 2s force-exits.
                                }
                                _ => {}
                            }
                        }
                        CeEvent::Mouse(m) => {
                            match m.kind {
                                MouseEventKind::ScrollUp => { log_scroll = log_scroll.saturating_add(2); }
                                MouseEventKind::ScrollDown => { log_scroll = log_scroll.saturating_sub(2); }
                                MouseEventKind::Down(_) => {
                                    let chain_idx = m.row.saturating_sub(2) as usize;
                                    if chain_idx < state.chains.len() {
                                        state.focused_chain = chain_idx;
                                    }
                                }
                                _ => {}
                            }
                        }
                        CeEvent::Resize(cols, rows) => {
                            if cols < MIN_COLS || rows < MIN_ROWS {
                                drop(guard);
                                eprintln!("[hm] terminal too small for TUI; falling back to streaming output");
                                return Ok(consume_to_end(&mut events).await);
                            }
                        }
                        _ => {}
                    }
                }

                if !needs_render && !fx.is_animating() {
                    continue;
                }
                needs_render = false;

                guard.terminal.draw(|f| {
                    let size = f.area();
                    let chunks = Layout::default()
                        .direction(Direction::Vertical)
                        .constraints([
                            Constraint::Length(2),
                            Constraint::Length(8),
                            Constraint::Min(0),
                            Constraint::Length(1),
                        ])
                        .split(size);

                    let row = Layout::default()
                        .direction(Direction::Horizontal)
                        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
                        .split(chunks[1]);

                    f.render_widget(Header { state: &state, theme: &theme, title: &opts.title }, chunks[0]);
                    f.render_widget(Graph { state: &state, theme: &theme }, row[0]);
                    f.render_widget(Timeline { state: &state, theme: &theme }, row[1]);
                    let filter_ref = if filter_open || !filter_buf.is_empty() {
                        Some(filter_buf.as_str())
                    } else {
                        None
                    };
                    f.render_widget(
                        LogPane {
                            state: &state,
                            theme: &theme,
                            scroll: log_scroll,
                            filter: filter_ref,
                        },
                        chunks[2],
                    );
                    f.render_widget(Footer { state: &state, theme: &theme }, chunks[3]);
                    if filter_open {
                        let fa = Rect::new(
                            chunks[2].x,
                            chunks[2].y + chunks[2].height.saturating_sub(1),
                            chunks[2].width,
                            1,
                        );
                        f.render_widget(Filter { theme: &theme, query: &filter_buf }, fa);
                    }
                    if help_open {
                        let w = 50.min(size.width.saturating_sub(4));
                        let h = 14.min(size.height.saturating_sub(4));
                        let r = Rect::new(
                            (size.width.saturating_sub(w)) / 2,
                            (size.height.saturating_sub(h)) / 2,
                            w,
                            h,
                        );
                        f.render_widget(Help { theme: &theme }, r);
                    }
                    let buf = f.buffer_mut();
                    fx.tick(buf, delta);
                }).map_err(TuiError::Io)?;
            }
            ev = events.recv() => {
                match ev {
                    Some(e @ TuiEvent::StepCacheHit { .. }) => {
                        needs_render = true;
                        fx.push_sparkle(Rect::new(0, 2, 40, 6));
                        state.apply(e);
                    }
                    Some(e @ TuiEvent::StepEnd { exit_code: 0, .. }) => {
                        needs_render = true;
                        fx.push_sparkle(Rect::new(0, 2, 40, 6));
                        state.apply(e);
                    }
                    Some(TuiEvent::BuildEnd { exit_code, duration_ms }) => {
                        state.apply(TuiEvent::BuildEnd { exit_code, duration_ms });
                        return finalise(&state, opts.summary_card, &theme, &mut guard).await;
                    }
                    Some(e) => {
                        needs_render = true;
                        state.apply(e);
                    }
                    None => return finalise(&state, opts.summary_card, &theme, &mut guard).await,
                }
            }
        }
    }
}

async fn finalise(
    state: &AppState,
    summary_card: bool,
    theme: &Theme,
    guard: &mut TermGuard,
) -> Result<i32, TuiError> {
    if summary_card {
        guard.terminal.draw(|f| {
            let size = f.area();
            f.render_widget(Summary { state, theme }, size);
        }).map_err(TuiError::Io)?;
        tokio::time::sleep(SUMMARY_HOLD).await;
    }
    Ok(state.exit_code.unwrap_or(0))
}

async fn consume_to_end(events: &mut mpsc::Receiver<TuiEvent>) -> i32 {
    let mut code = 0;
    while let Some(ev) = events.recv().await {
        if let TuiEvent::BuildEnd { exit_code, .. } = ev {
            code = exit_code;
        }
    }
    code
}
