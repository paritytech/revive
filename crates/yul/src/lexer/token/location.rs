//! The lexical token location.

use serde::Deserialize;
use serde::Serialize;

/// The token location in the source code file.
#[derive(Debug, Serialize, Deserialize, Clone, Copy, Eq)]
pub struct Location {
    /// The line number, starting from 1.
    pub line: u32,
    /// The column number, starting from 1.
    pub column: u32,
}

impl Default for Location {
    fn default() -> Self {
        Self { line: 1, column: 1 }
    }
}

impl Location {
    /// Creates a default location.
    pub fn new(line: u32, column: u32) -> Self {
        Self { line, column }
    }

    /// Mutates the location by shifting the original one down by `lines` and
    /// setting the column to `column`.
    pub fn shift_down(&mut self, lines: u32, column: u32) {
        if lines == 0 {
            self.shift_right(column);
            return;
        }

        self.line += lines;
        self.column = column;
    }

    /// Mutates the location by shifting the original one rightward by `columns`.
    pub fn shift_right(&mut self, columns: u32) {
        self.column += columns;
    }
}

impl PartialEq for Location {
    fn eq(&self, other: &Self) -> bool {
        self.line == other.line && self.column == other.column
    }
}

impl std::fmt::Display for Location {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.line, self.column)
    }
}
