//! Rich diagnostics via miette.

use flake_ast::{Source, Span};
use miette::{LabeledSpan, NamedSource, Report};

/// Print a miette diagnostic to stderr.
pub fn emit(source: &Source, span: Span, message: &str) {
    let start = span.start as usize;
    let end = (span.end as usize).max(start + 1);
    let report: Report = miette::miette!(
        labels = vec![LabeledSpan::at(start..end, "here")],
        "{message}"
    )
    .with_source_code(NamedSource::new(
        source.name().to_string(),
        source.text().to_string(),
    ));
    eprintln!("{report:?}");
}

pub fn emit_message(message: &str) {
    let report: Report = miette::miette!("{message}");
    eprintln!("{report:?}");
}
