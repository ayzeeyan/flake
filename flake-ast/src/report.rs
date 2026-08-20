//! Lightweight diagnostic rendering used until rich diagnostics land.

use crate::span::{Source, Span};

/// Render a caret diagnostic for `span` in `source`.
#[must_use]
pub fn render(source: &Source, span: Span, kind: &str, message: &str) -> String {
    let start = source.locate(span.start);
    let line_text = source.line_text(start.line).unwrap_or("");
    let col = start.column.max(1) as usize;
    let mut caret_len = span.len().max(1) as usize;
    let remaining = line_text.len().saturating_sub(col.saturating_sub(1));
    if remaining > 0 {
        caret_len = caret_len.min(remaining.max(1));
    } else {
        caret_len = 1;
    }

    let mut out = String::new();
    out.push_str(&format!("{kind}: {message}\n"));
    out.push_str(&format!(" --> {}:{}:{}\n", source.name(), start.line, start.column));
    out.push_str("  |\n");
    out.push_str(&format!("{:>4} | {}\n", start.line, line_text));
    out.push_str(&format!(
        "  | {}{}\n",
        " ".repeat(col.saturating_sub(1)),
        "^".repeat(caret_len.max(1))
    ));
    out
}
