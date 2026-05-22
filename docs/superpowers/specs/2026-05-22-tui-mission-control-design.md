# `hm` Mission Control TUI — Design

**Status:** approved 2026-05-22
**Owner:** marko@simci.dev
**Goal:** Replace the default plain-text TTY output of `hm run`, `hm dev up`,
and `hm cloud build watch` with a beautiful, animated, host-side TUI that
is the easiest and most screenshot-worthy way to run a Harmont deployment.

The TUI is also the marketing surface — every frame is something a viewer
might quote-tweet, so engagement is a first-class non-functional requirement
alongside correctness and performance.

---

## 1. Architecture

### 1.1 Runtime placement

The TUI lives **inside the `hm` binary** at `crates/hm/src/tui/`. It is not a
WASM output plugin. Rationale:

- ratatui needs raw mode, the alternate screen, mouse capture, resize events,
  and ideally 60fps frame submission. The existing `OutputFormatter` capability
  is pure `on_event(BuildEvent)` running in an Extism sandbox with only
  `hm_write_stdout` / `hm_write_stderr` host fns. Bridging that to ratatui
  would require ~10 new host fns and would still pay a per-call WASM tax on
  every animation frame.
- The WASM-plugin output path (`hm-plugin-output-human`, `hm-plugin-output-json`)
  is **not removed**. It remains the non-TTY default and the `--format human`
  / `--format json` opt-out. The new TUI is a third sibling, selected by TTY
  detection.

### 1.2 Event-source abstraction

All three command surfaces feed the TUI through a single, typed channel:

```rust
// crates/hm/src/tui/event.rs
pub enum TuiEvent {
    BuildStart { run_id: Uuid, plan: PlanSummary, started_at: DateTime<Utc> },
    ChainQueued { chain_idx: usize, label: String, parent: Option<usize> },
    StepStart { step_id: Uuid, chain_idx: usize, runner: String, image: Option<String>, label: String },
    StepLog { step_id: Uuid, stream: StdStream, line: String, ts: DateTime<Utc> },
    StepCacheHit { step_id: Uuid, key: String, tag: String },
    StepEnd { step_id: Uuid, exit_code: i32, duration_ms: u64 },
    ChainFailed { chain_idx: usize, failed_step_key: String, exit_code: i32, message: String },
    BuildEnd { exit_code: i32, duration_ms: u64 },

    // dev-only
    DeployStatus { deploy_id: String, label: String, state: DeployState, restarts: u32, uptime_ms: u64 },
    DeployLog { deploy_id: String, stream: StdStream, line: String, ts: DateTime<Utc> },
}

pub enum DeployState { Starting, Healthy, Unhealthy, Restarting, Stopped }
```

`TuiEvent` is a **host-only** type. It is not on the plugin wire — wire types
stay in `hm-plugin-protocol` and remain frozen. The TUI translates inbound
data into `TuiEvent` at the adapter boundary so the rest of the TUI module
sees one event vocabulary.

Three adapters live under `crates/hm/src/tui/source/`:

- `local.rs` — subscribes via `orchestrator::events::Bus::subscribe()` (the
  existing `tokio::sync::broadcast` used by the WASM output subscriber today)
  and maps `BuildEvent → TuiEvent`. The mapping is 1:1 for build variants;
  `Deploy*` variants are never emitted.
- `dev.rs` — wraps the dev daemon status source already used by `hm dev`. Ticks
  on a 500ms interval; emits a `DeployStatus` per known deploy + tails each
  deploy's log stream into `DeployLog`. `BuildStart` / `BuildEnd` are
  synthesized at session begin / end with `chain_count = deploy_count`.
- `cloud.rs` — bridges the cloud watch loop, which today lives **inside the
  `hm-plugin-cloud` WASM plugin** at
  `crates/hm-plugin-cloud/src/verbs/build.rs::watch`. To route polled cloud
  state into the host TUI without lifting the watch loop out of WASM, we
  add one new host fn: `hm_build_event_emit(json_bytes) -> ()`. The payload
  is a JSON-serialized wire `BuildEvent` (already defined in
  `hm-plugin-protocol`, so the wire surface gains a transport channel but no
  new types). The host implementation pushes the deserialized `BuildEvent`
  into the same mpsc channel that `local.rs` uses, after the standard
  `BuildEvent → TuiEvent` translation. The cloud plugin's `watch` calls
  `hm_build_event_emit` once per state diff plus synthetic `BuildStart` /
  `BuildEnd` bookends. This host fn is the only protocol-level addition
  this design requires; it is also reusable by any future plugin that needs
  to drive the host TUI from a poll/stream loop.

### 1.3 Entry point

```rust
// crates/hm/src/tui/mod.rs
pub async fn run(
    mut events: mpsc::Receiver<TuiEvent>,
    opts: TuiOptions,
) -> Result<i32, TuiError> { /* … */ }

pub struct TuiOptions {
    pub fx_enabled: bool,
    pub summary_card: bool,
    pub title: String,         // "hm run", "hm dev", "hm cloud build watch"
}
```

Each command's existing dispatch picks one of three paths:

1. `--format json` → existing JSON plugin (unchanged).
2. `--format human` **or** non-TTY **or** `--no-tui` → existing human plugin
   (unchanged).
3. Otherwise → `tui::run(...)`.

The return value from `tui::run` is the build's exit code; the command propagates
it as today.

### 1.4 Process lifecycle

- On entry: enter the alternate screen, enable raw mode, enable mouse capture,
  start a `tokio::time::interval(Duration::from_millis(16))` frame ticker.
- On exit (any path — success, error, Ctrl-C, panic): a single guard restores
  the terminal in `Drop` (disable mouse, leave alt screen, disable raw mode,
  show cursor). The guard is wrapped around the entire `tui::run` call so a
  panic inside ratatui still restores the terminal before the panic message
  prints. We additionally install a `std::panic::set_hook` that calls the
  same restore function before delegating to the previous hook, so panics in
  background tasks don't leave a broken terminal.
- Cancellation: `Ctrl-C` first asks the orchestrator to cancel (existing
  `cancel.rs` channel); a second `Ctrl-C` within 2s exits immediately.

---

## 2. UI Layout

The TUI is a **fixed three-zone layout**. We deliberately do not ship a
window manager / movable panes — every screenshot looks the same shape, which
helps brand recognition.

```
┌─ HARMONT ──── run 4f2a · main · 00:42 ─ 3 chains · 2/9 done ─────┐  ← header (2 rows)
│ graph              │ timeline                                     │
│  ●─┬─●─●           │  c1 ████████████░░░░░ test    4.2s  pass     │  ← graph + timeline
│    ├─◆─●           │  c2 ███████░░░░░░░░░░ build ⚡ 1.1s  cache   │     row, split 40/60
│    └─◆             │  c3 ██░░░░░░░░░░░░░░░ lint    0.4s  run      │     (vertical: ~30%)
│                                                                   │
│ log · c1 · test                                                   │  ← log pane
│  $ cargo test --workspace                                         │     (remaining ~65%)
│  running 142 tests                                                │
│  test orchestrator::cache::hit … ok                               │
│  …                                                                │
└─ [tab] chain · [l] logs · [/] filter · [q] quit ─────────────────┘  ← footer (1 row)
```

### 2.1 Zones

**Header (`widgets/header.rs`)** — 2 rows. Line 1: `HARMONT` wordmark (small,
bold, gradient via tachyonfx hsl-shift only during build), then `run <short
run_id>`, branch, elapsed time, `N chains · K/N done` counter. Line 2: blank
separator with bottom border.

**Graph (`widgets/graph.rs`)** — left ~40% of middle row. Renders the chain
DAG with chains as horizontal lanes and steps as glyphs:

- `●` step (pending) · `◐` step (running) · `◇` step (passed) · `◆` step
  (cached hit) · `✖` step (failed)
- `┬` `├` `└` `─` ASCII connectors for forks/joins.

Layout algorithm: simple longest-chain topo sort, one row per chain, fork
glyphs at the divergence column. For plans with > N chains where N is
`viewport_rows - reserved`, scroll vertically; show `…` collapse indicator.

**Timeline (`widgets/timeline.rs`)** — right ~60% of middle row. Gantt-style
horizontal bars per chain, color-coded by status:

- bar fill: gray pending · cyan running · green passed · yellow cached ·
  red failed
- right-aligned: step label, duration, status pill.
- x-axis: 0 → max(elapsed, slowest expected). Bars grow in place as work runs.
- For long runs the x-axis auto-rescales; the rescale is animated with a
  100ms ease so the demo reads as smooth.

**Log (`widgets/log.rs`)** — remaining height. Tails the currently-focused
chain's most-recent step. Each line is `[timestamp] <line>` with stderr
prefixed by a dim `! `. Scrollback buffer: ring of 2000 lines per step. Auto-
scrolls to bottom unless the user has scrolled up (then a `↓ more` indicator
appears in the footer). `/` opens an inline regex filter on the log buffer.

**Footer (`widgets/footer.rs`)** — single row. Left: keybinding hints. Right:
status pill summary (`9 passed`, `1 cached`, `0 failed`).

### 2.2 Focus and interaction

The "focused chain" is a single index into the chain list. `Tab` /
`Shift-Tab` cycle it; clicking a chain row in graph or timeline sets it; the
log pane mirrors it. The focused chain row gets a brighter border color.

Keybindings (canonical):

| Key | Action |
|---|---|
| `q`, `Esc` | Quit (asks orchestrator to cancel if still running) |
| `Ctrl-C` | First press: cancel run. Second within 2s: force-exit. |
| `Tab` / `Shift-Tab` | Cycle focused chain |
| `↑` / `↓` / wheel | Scroll log |
| `PgUp` / `PgDn` | Page-scroll log |
| `g` / `G` | Jump to top / bottom of log |
| `l` | Toggle log pane expand (hides graph+timeline for full-height log) |
| `/` | Open log filter |
| `?` | Toggle help overlay |
| click chain row | Focus that chain |
| click step glyph | Focus the chain containing it |

### 2.3 Final summary card

After `BuildEnd`, the live layout is replaced by a centered card (auto-sized
to ≥60×16, capped at 80×24) for 2 seconds **or** until any keypress:

```
                  ▄▄▄▄  HARMONT  ▄▄▄▄
                  build complete

           total          42.3s
           chains         3
           steps          9 passed · 1 cached · 0 failed
           cache hit %    33%
           slowest        test (4.2s)

           durations      ▁▂▃▄▅▄▃▂▁

           ↗ harmont.dev/build/4f2a
```

`tui-big-text` renders the wordmark line; the rest is plain widgets. Failed
builds replace the green banner with a red `build failed — c1: cargo test
exited 1`.

The summary card is **the** screenshot frame. The CI `vhs` tape (see §5)
exits at this frame so the resulting GIF/PNG loops to it.

---

## 3. Visual Effects

### 3.1 Effects budget

- **Frame loop**: 60fps tick (`tokio::time::interval(16ms)`). We render only
  when (a) the AppState dirty bit is set, or (b) any animation is active.
  Idle CPU at steady state is ≤ 1%.
- **Effect inventory** (all from `tachyonfx`):
  - `sparkle`: 80ms, glyph-localized, on `StepCacheHit` and successful
    `StepEnd`. Max 1 active per event; further events drop if >5 queued.
  - `fade_in`: 120ms, on each new chain row appearance.
  - `hsl_shift`: continuous shimmer on the `HARMONT` wordmark while a build
    is running. Stops at `BuildEnd`.
  - `slide_in_from_right`: 200ms, summary card entry.
- **No** confetti, no scanlines, no CRT glow. Mission Control is the chosen
  aesthetic — these would clash.

### 3.2 Disabling effects

Effects disable automatically when any of:

- `NO_COLOR` env var is set
- `--no-fx` flag is passed (new global flag)
- stdout is not a TTY (the TUI itself wouldn't run in this case, but
  belt-and-braces inside `TuiOptions`)
- the terminal reports fewer than 256 colors

When effects are off the layout is identical, just static.

### 3.3 Theme

Single theme; no theme switcher (YAGNI). Palette:

- background: terminal default
- borders: gray 244 (dim) / cyan 51 on focused chain
- accent: harmont gradient — cyan 51 → blue 33 (used by `hsl_shift` on
  wordmark)
- status: green 42 (pass) · yellow 220 (cache) · red 196 (fail) · cyan 51
  (run) · gray 244 (pending)

Defined in `tui/theme.rs` as a single `Theme` struct. Constructed once at
TUI start; passed by reference into every widget. Not a runtime-mutable
state.

---

## 4. Activation and fallback

### 4.1 Decision rules

For `hm run`, `hm dev up`, `hm cloud build watch`:

```
if format == "json"                          → JSON plugin
elif format == "human"                       → human plugin
elif !is_tty(stdout)                         → human plugin
elif --no-tui                                → human plugin
elif TERM == "dumb"                          → human plugin
elif viewport < 60 cols or < 20 rows         → human plugin (with a one-line
                                               "terminal too small for TUI"
                                               notice on stderr)
else                                         → TUI
```

`--no-tui` is a new global flag (alongside `--no-color`). `--format` defaults
remain unchanged — the TUI is *not* a `--format` value because it isn't a
WASM output plugin.

### 4.2 Resize handling

On `Resize` event from crossterm:

- If new viewport < 60×20: tear down TUI, fall back to human formatter
  mid-run by re-attaching the WASM output subscriber. Print a one-line
  notice on stderr. (This is a rare corner — most users don't resize during
  a build — but the path exists so we never leave a broken render.)
- Otherwise: clear and re-layout. The layout is responsive — graph hides
  if cols < 90; timeline labels truncate before chain rows hide.

### 4.3 Non-build commands

`hm version`, `hm plugin list`, etc. are unaffected. The TUI only attaches
to `run`, `dev up`, and `cloud build watch`.

---

## 5. Testing and demo artifacts

### 5.1 Layered tests

- **Reducer tests** (`crates/hm/src/tui/app.rs` `#[cfg(test)] mod tests`) —
  Pure-function tests of `AppState::apply(event) -> AppState`. No terminal,
  no async. Covers: chain ordering, step status transitions, focus
  invariants, log buffer ring, filter behavior.
- **Snapshot tests** (`crates/hm/tests/tui_snapshots.rs`) — Render the
  ratatui `Frame` into a `Buffer`, serialize the buffer cells as text + style
  per cell, snapshot with `insta`. Snapshots cover: empty state, one-chain
  running, multi-chain with cache hit, one-chain failed, summary card
  (pass), summary card (fail).
- **Resize tests** — feed a sequence of `Resize` events through the test
  harness and snapshot the resulting frame at each.
- **No** end-to-end terminal tests in CI — those are flaky. Visual
  regressions are caught by snapshots; the demo tape (next section) is the
  human-eye check.

### 5.2 Demo artifacts

- `docs/demo/run.tape` — a [vhs](https://github.com/charmbracelet/vhs) tape
  that runs `hm run` against `examples/rust/` and freezes on the summary
  card. Committed alongside the generated `run.gif`.
- `docs/demo/dev.tape` — same for `hm dev up` against a small example.
- `docs/demo/cloud.tape` — same for `hm cloud build watch` (recorded against
  a staging build; the tape ships a captured frame because the cloud API
  isn't replayable).
- CI workflow `.github/workflows/demo.yml` — smoke-runs `run.tape` on PRs to
  ensure the TUI still renders end-to-end. Does NOT fail on visual
  differences (vhs is non-deterministic enough that we'd flake); only fails
  if the tape errors out.

The committed `run.gif` is what the README embeds and what every Twitter
post links to. This is a load-bearing artifact for the engagement goal.

---

## 6. File map

### Created

- `crates/hm/src/tui/mod.rs` — public entry: `pub async fn run(...)`.
- `crates/hm/src/tui/event.rs` — `TuiEvent`, `DeployState` enums.
- `crates/hm/src/tui/app.rs` — `AppState`, reducer (`apply(event)`),
  derived view (focused chain, durations).
- `crates/hm/src/tui/source/mod.rs` — `EventSource` trait + factory.
- `crates/hm/src/tui/source/local.rs` — local build adapter.
- `crates/hm/src/tui/source/dev.rs` — dev daemon adapter.
- `crates/hm/src/tui/source/cloud.rs` — cloud watch adapter.
- `crates/hm/src/tui/widgets/mod.rs` — pub use of all widgets.
- `crates/hm/src/tui/widgets/header.rs`
- `crates/hm/src/tui/widgets/graph.rs`
- `crates/hm/src/tui/widgets/timeline.rs`
- `crates/hm/src/tui/widgets/log.rs`
- `crates/hm/src/tui/widgets/footer.rs`
- `crates/hm/src/tui/widgets/summary.rs`
- `crates/hm/src/tui/theme.rs` — `Theme` struct + constructor.
- `crates/hm/src/tui/fx.rs` — tachyonfx effect builders + budget enforcement.
- `crates/hm/src/tui/term.rs` — terminal-setup guard (alt screen, raw mode,
  mouse, panic hook).
- `crates/hm/tests/tui_snapshots.rs` — insta snapshot tests.
- `docs/demo/run.tape`, `docs/demo/dev.tape`, `docs/demo/cloud.tape`.
- `.github/workflows/demo.yml`.

### Modified

- `crates/hm/Cargo.toml` — add `ratatui = "0.30"`, `crossterm = "0.29"`,
  `tachyonfx = "0.20"`, `tui-big-text = "0.8"`, `[dev-dependencies] insta`.
- `crates/hm/src/cli.rs` — add `--no-tui` and `--no-fx` global flags.
- `crates/hm/src/lib.rs` — `pub mod tui;`.
- `crates/hm/src/commands/run/mod.rs` — TTY-detect branch; call
  `tui::run(source::local::stream(...), opts)` when applicable.
- `crates/hm/src/commands/dev/up.rs` — same, with `source::dev`.
- `crates/hm-plugin-cloud/src/verbs/build.rs` — replace `watch`'s current
  stdout-printing loop with calls to `hm_build_event_emit` (host-side
  TUI consumes), keeping a stdout-printing fallback path for `--format
  human` / non-TTY.
- `crates/hm-plugin-cloud/src/lib.rs` (and its host-fn import block) — declare
  `hm_build_event_emit` in the imported host-fns.
- `crates/hm/src/plugin/host_fns/` — implement the `hm_build_event_emit`
  host fn: deserialize JSON `BuildEvent` and forward it to the TUI
  mpsc sender registered in plugin context.
- `crates/hm-plugin-protocol/src/host_abi.rs` — declare the host fn name
  constant (no new wire types).
- `README.md` — embed the new `run.gif`.

### Not touched

- `crates/hm-plugin-protocol/` wire **types** — no new structs, no new
  enum variants. Only one host-fn name constant added (see Modified).
- `crates/hm-plugin-output-human/` — still ships, still loaded, still the
  non-TTY default.
- `crates/hm-plugin-output-json/` — unchanged.
- The `OutputFormatter` capability and `hm_output_on_event` host fn —
  unchanged.
- `HM_PLUGIN_API_VERSION` — does **not** bump. The new host fn is additive
  and optional; the cloud plugin (which uses it) ships embedded in the same
  binary so version skew is impossible.

---

## 7. Non-goals

- Multi-pane / window manager UX (lazygit-style movable panes).
- Theme switcher / config-driven palettes.
- Plugins drawing into the TUI (the "hybrid" architecture from
  brainstorming). Revisit if a plugin author asks for it.
- Boot intro animation. Cut from v1 to keep the path-to-screenshot fast.
- Inline raster logos (Kitty/Sixel). Cut from v1; can be added behind a
  feature flag later.
- TUI on Windows. Crossterm supports it but we're not testing it in CI;
  treat as best-effort.

---

## 8. Open risks

- **tachyonfx + ratatui 0.30 compatibility.** tachyonfx 0.20 targets
  ratatui 0.30 per its changelog, but we should verify on day 1 of
  implementation. Fallback: pin tachyonfx to whatever last version paired
  with the ratatui we adopt, even if it means staying on ratatui 0.29.
- **Mouse capture conflicts with terminal text selection.** Modifier-key
  selection (Shift+drag on most terms) still works; document this in
  `--help`.
- **vhs in CI** is non-deterministic. The CI smoke test only checks
  process-exit-zero, not pixel diffs.
- **Broadcast lag** in the local event bus already surfaces a `warn!`; the
  TUI must not deadlock when the adapter task falls behind. The mpsc channel
  between adapter and TUI is bounded (capacity 1024); the adapter drops
  `StepLog` events when full (and emits a single `<lagged>` synthetic event)
  rather than blocking the orchestrator.
- **External (third-party) plugins** that re-implement output today won't
  benefit from the TUI. Their stdout is captured behind the alt-screen and
  silently dropped while the TUI runs. We document this in the plugin
  authoring guide and offer `hm_build_event_emit` as the migration path. If
  enough third-party plugins need it before we ship, gate the TUI behind
  `--tui` instead of TTY-detect for a release.
