//! Terminal setup / restore guard. Owning a `TermGuard` switches the
//! terminal into alt screen + raw mode + mouse capture; dropping it
//! restores the previous state, even on panic.

use std::io::{self, Stdout};

use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{
        disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
    },
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

pub type TuiTerminal = Terminal<CrosstermBackend<Stdout>>;

/// Holds the terminal in TUI mode. Restores on drop or panic.
pub struct TermGuard {
    pub terminal: TuiTerminal,
}

impl std::fmt::Debug for TermGuard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TermGuard").finish()
    }
}

impl TermGuard {
    /// Enter alt screen + raw mode + mouse capture and return the guard.
    ///
    /// # Errors
    /// Returns an `io::Error` if any of the terminal setup steps fail.
    pub fn enter() -> io::Result<Self> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
        let backend = CrosstermBackend::new(stdout);
        let terminal = Terminal::new(backend)?;
        install_panic_hook();
        Ok(Self { terminal })
    }
}

impl Drop for TermGuard {
    fn drop(&mut self) {
        let _ = restore();
    }
}

fn restore() -> io::Result<()> {
    let mut stdout = io::stdout();
    execute!(stdout, LeaveAlternateScreen, DisableMouseCapture)?;
    disable_raw_mode()?;
    Ok(())
}

fn install_panic_hook() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let prev = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            let _ = restore();
            prev(info);
        }));
    });
}
