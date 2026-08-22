//! Rich diagnostics via miette.

use flake_ast::{Source, Span};
use miette::{LabeledSpan, NamedSource, Report};

/// Print a miette diagnostic to stderr.
pub fn emit(source: &Source, span: Span, message: &str) {
    let start = span.start as usize;
    let end = (span.end as usize).max(start + 1);
    let (main, help) = match message.split_once("\nhelp: ") {
        Some((main, help)) => (main, Some(help.trim())),
        None => (message, None),
    };
    let labeled = vec![LabeledSpan::at(start..end, "here")];
    let report: Report = if let Some(help) = help {
        miette::miette!(labels = labeled, help = help, "{main}")
    } else {
        miette::miette!(labels = labeled, "{main}")
    }
    .with_source_code(NamedSource::new(source.name(), source.text().to_string()));
    eprintln!("{report:?}");
}

pub fn emit_message(message: &str) {
    let report: Report = miette::miette!("{message}");
    eprintln!("{report:?}");
}
