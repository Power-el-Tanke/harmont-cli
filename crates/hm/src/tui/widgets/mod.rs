//! Mission Control widget set. All widgets are stateless: they read
//! `&AppState` + `&Theme` and write into a `Buffer`.

pub mod filter;
pub mod footer;
pub mod graph;
pub mod header;
pub mod help;
pub mod log;
pub mod summary;
pub mod timeline;

/// Format a `Buffer` as one row per line for snapshot tests.
#[cfg(test)]
#[allow(clippy::missing_const_for_fn)]
pub(crate) fn buffer_to_string(buf: &ratatui::buffer::Buffer) -> String {
    let mut out = String::new();
    let area = buf.area();
    for y in 0..area.height {
        for x in 0..area.width {
            out.push_str(buf[(x, y)].symbol());
        }
        out.push('\n');
    }
    out
}
