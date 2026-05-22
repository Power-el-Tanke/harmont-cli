//! Effect budget + factory. Wraps tachyonfx so the rest of the TUI
//! sees a small, stable surface.

use std::collections::VecDeque;
use std::time::Duration;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;
use tachyonfx::{fx, Effect, EffectTimer, Interpolation, Motion};

/// Maximum simultaneous effects. Beyond this, new effect requests are
/// dropped silently — spec budget §3.1.
pub const MAX_QUEUED: usize = 5;

pub struct ActiveEffect {
    pub effect: Effect,
    pub area: Rect,
}

#[allow(clippy::missing_fields_in_debug, reason = "Effect is not Debug; area is sufficient for diagnostics")]
impl std::fmt::Debug for ActiveEffect {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ActiveEffect").field("area", &self.area).finish()
    }
}

#[derive(Debug, Default)]
pub struct FxQueue {
    queue: VecDeque<ActiveEffect>,
    enabled: bool,
}

impl FxQueue {
    #[must_use]
    pub const fn new(enabled: bool) -> Self {
        Self { queue: VecDeque::new(), enabled }
    }

    pub fn push_sparkle(&mut self, area: Rect) {
        if !self.enabled || self.queue.len() >= MAX_QUEUED {
            return;
        }
        let timer = EffectTimer::from_ms(80, Interpolation::Linear);
        let effect = fx::sweep_in(Motion::LeftToRight, 6, 0, Color::Black, timer);
        self.queue.push_back(ActiveEffect { effect, area });
    }

    pub fn push_fade_in(&mut self, area: Rect) {
        if !self.enabled || self.queue.len() >= MAX_QUEUED {
            return;
        }
        let timer = EffectTimer::from_ms(120, Interpolation::Linear);
        let effect = fx::fade_from_fg(Color::Black, timer);
        self.queue.push_back(ActiveEffect { effect, area });
    }

    pub fn push_slide_in(&mut self, area: Rect) {
        if !self.enabled || self.queue.len() >= MAX_QUEUED {
            return;
        }
        let timer = EffectTimer::from_ms(200, Interpolation::QuadOut);
        let effect = fx::sweep_in(Motion::RightToLeft, 12, 0, Color::Black, timer);
        self.queue.push_back(ActiveEffect { effect, area });
    }

    #[must_use]
    pub fn is_animating(&self) -> bool {
        !self.queue.is_empty()
    }

    /// Drive every queued effect by `delta` and drop completed ones.
    /// Call once per frame.
    pub fn tick(&mut self, buf: &mut Buffer, delta: Duration) {
        // tachyonfx::Duration is a custom u32-millisecond type; std::time::Duration
        // converts via From when the "std" feature (default) is active.
        let tfx_delta: tachyonfx::Duration = delta.into();
        self.queue.retain_mut(|a| {
            a.effect.process(tfx_delta, buf, a.area);
            !a.effect.done()
        });
    }
}
