use std::fmt;

/// Source location span tracking line, column, byte offset, and length.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    /// 1-indexed line number.
    pub line: usize,
    /// 1-indexed column number.
    pub column: usize,
    /// 0-indexed byte offset into the source.
    pub offset: usize,
    /// Byte length of the spanned region.
    pub length: usize,
}

impl Span {
    /// Creates a new `Span` with the given position information.
    #[must_use]
    pub const fn new(line: usize, column: usize, offset: usize, length: usize) -> Self {
        Self {
            line,
            column,
            offset,
            length,
        }
    }

    /// Creates a dummy span with zeroed fields for internal use.
    #[must_use]
    pub const fn dummy() -> Self {
        Self {
            line: 0,
            column: 0,
            offset: 0,
            length: 0,
        }
    }

    /// Merges two spans into one that covers both regions.
    #[must_use]
    pub const fn merge(start: Self, end: Self) -> Self {
        Self {
            line: start.line,
            column: start.column,
            offset: start.offset,
            length: (end.offset + end.length).saturating_sub(start.offset),
        }
    }
}

impl fmt::Display for Span {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}, column {}", self.line, self.column)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_span_new() {
        let s = Span::new(1, 5, 10, 3);
        assert_eq!(s.line, 1);
        assert_eq!(s.column, 5);
        assert_eq!(s.offset, 10);
        assert_eq!(s.length, 3);
    }

    #[test]
    fn test_span_dummy() {
        let s = Span::dummy();
        assert_eq!(s.line, 0);
        assert_eq!(s.column, 0);
        assert_eq!(s.offset, 0);
        assert_eq!(s.length, 0);
    }

    #[test]
    fn test_span_merge() {
        let a = Span::new(1, 1, 0, 5);
        let b = Span::new(1, 6, 5, 3);
        let merged = Span::merge(a, b);
        assert_eq!(merged.offset, 0);
        assert_eq!(merged.length, 8);
    }

    #[test]
    fn test_span_display() {
        let s = Span::new(3, 7, 20, 1);
        assert_eq!(format!("{s}"), "line 3, column 7");
    }
}
