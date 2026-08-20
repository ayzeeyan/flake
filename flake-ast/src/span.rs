//! Byte-offset source spans and line/column mapping.

use std::fmt;
use std::ops::Range;

/// A half-open byte range into a source file: `[start, end)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Span {
    pub start: u32,
    pub end: u32,
}

impl Span {
    pub const DUMMY: Span = Span { start: 0, end: 0 };

    #[must_use]
    pub fn new(start: u32, end: u32) -> Self {
        debug_assert!(start <= end, "span start ({start}) > end ({end})");
        Self { start, end }
    }

    #[must_use]
    pub fn point(offset: u32) -> Self {
        Self {
            start: offset,
            end: offset,
        }
    }

    #[must_use]
    pub fn is_dummy(self) -> bool {
        self.start == 0 && self.end == 0
    }

    #[must_use]
    pub fn is_empty(self) -> bool {
        self.start == self.end
    }

    #[must_use]
    pub fn len(self) -> u32 {
        self.end.saturating_sub(self.start)
    }

    #[must_use]
    pub fn range(self) -> Range<usize> {
        self.start as usize..self.end as usize
    }

    #[must_use]
    pub fn merge(self, other: Self) -> Self {
        if self.is_dummy() {
            return other;
        }
        if other.is_dummy() {
            return self;
        }
        Self {
            start: self.start.min(other.start),
            end: self.end.max(other.end),
        }
    }

    #[must_use]
    pub fn contains(self, offset: u32) -> bool {
        offset >= self.start && offset < self.end
    }
}

impl fmt::Display for Span {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}..{}", self.start, self.end)
    }
}

/// 1-based line and column (column is in Unicode scalar values, not bytes).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LineCol {
    pub line: u32,
    pub column: u32,
}

impl fmt::Display for LineCol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.line, self.column)
    }
}

/// A named source buffer with a cached line-start index for diagnostics.
#[derive(Debug, Clone)]
pub struct Source {
    name: String,
    text: String,
    line_starts: Vec<u32>,
}

impl Source {
    #[must_use]
    pub fn new(name: impl Into<String>, text: impl Into<String>) -> Self {
        let text = text.into();
        let line_starts = line_starts(&text);
        Self {
            name: name.into(),
            text,
            line_starts,
        }
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.text.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    #[must_use]
    pub fn slice(&self, span: Span) -> &str {
        let range = span.range();
        let end = range.end.min(self.text.len());
        let start = range.start.min(end);
        &self.text[start..end]
    }

    /// Locate the 1-based line/column of a byte offset.
    #[must_use]
    pub fn locate(&self, offset: u32) -> LineCol {
        let mut offset = offset.min(self.text.len() as u32) as usize;
        while offset > 0 && !self.text.is_char_boundary(offset) {
            offset -= 1;
        }
        let offset = offset as u32;
        let line_idx = match self.line_starts.binary_search(&offset) {
            Ok(i) => i,
            Err(i) => i.saturating_sub(1),
        };
        let line_start = self.line_starts[line_idx] as usize;
        let column = self.text[line_start..offset as usize].chars().count() as u32 + 1;
        LineCol {
            line: line_idx as u32 + 1,
            column,
        }
    }

    #[must_use]
    pub fn locate_span(&self, span: Span) -> (LineCol, LineCol) {
        (self.locate(span.start), self.locate(span.end))
    }

    /// Byte offset of the start of the 1-based `line`.
    #[must_use]
    pub fn line_start(&self, line: u32) -> Option<u32> {
        self.line_starts.get(line.saturating_sub(1) as usize).copied()
    }

    /// Text of the 1-based `line`, without its trailing newline.
    #[must_use]
    pub fn line_text(&self, line: u32) -> Option<&str> {
        let idx = line.saturating_sub(1) as usize;
        let start = *self.line_starts.get(idx)? as usize;
        let end = self
            .line_starts
            .get(idx + 1)
            .map(|off| *off as usize)
            .unwrap_or(self.text.len());
        let mut slice = &self.text[start..end];
        if let Some(stripped) = slice.strip_suffix('\n') {
            slice = stripped;
            if let Some(stripped) = slice.strip_suffix('\r') {
                slice = stripped;
            }
        } else if let Some(stripped) = slice.strip_suffix('\r') {
            slice = stripped;
        }
        Some(slice)
    }

    #[must_use]
    pub fn line_count(&self) -> usize {
        // `line_starts` always has at least the initial 0, and a trailing
        // newline introduces an extra empty line, matching typical editors.
        if self.text.is_empty() {
            1
        } else {
            self.line_starts.len()
        }
    }
}

fn line_starts(text: &str) -> Vec<u32> {
    let mut starts = vec![0];
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'\n' => {
                starts.push((i + 1) as u32);
                i += 1;
            }
            b'\r' => {
                let next = if bytes.get(i + 1) == Some(&b'\n') {
                    i + 2
                } else {
                    i + 1
                };
                starts.push(next as u32);
                i = next;
            }
            _ => i += 1,
        }
    }
    starts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_takes_extents() {
        let a = Span::new(2, 5);
        let b = Span::new(4, 9);
        assert_eq!(a.merge(b), Span::new(2, 9));
        assert_eq!(Span::DUMMY.merge(b), b);
    }

    #[test]
    fn locate_crlf_and_columns() {
        let src = Source::new("t.flk", "ab\r\ncé\n");
        assert_eq!(src.locate(0), LineCol { line: 1, column: 1 });
        assert_eq!(src.locate(2), LineCol { line: 1, column: 3 });
        assert_eq!(src.locate(4), LineCol { line: 2, column: 1 });
        // 'é' is two bytes; column still advances by one scalar.
        // `é` occupies bytes 5..7; column still advances by one scalar.
        assert_eq!(src.locate(7), LineCol { line: 2, column: 3 });
        assert_eq!(src.line_text(1), Some("ab"));
        assert_eq!(src.line_text(2), Some("cé"));
    }

    #[test]
    fn empty_source_has_one_line() {
        let src = Source::new("empty", "");
        assert_eq!(src.line_count(), 1);
        assert_eq!(src.locate(0), LineCol { line: 1, column: 1 });
    }
}
