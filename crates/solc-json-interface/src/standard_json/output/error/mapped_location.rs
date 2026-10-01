//! The mapped error location.

use crate::standard_json::output::error::source_location::SourceLocation;

/// The mapped error location.
///
/// It can be resolved from `solc` AST error location if the source code is provided.
#[derive(Debug)]
pub struct MappedLocation<'a> {
    /// The source file path.
    pub path: String,
    /// The line number.
    pub line: Option<usize>,
    /// The column number.
    pub column: Option<usize>,
    /// The error area length.
    pub length: Option<usize>,
    /// The source code line to print.
    pub source_code_line: Option<&'a str>,
}

impl<'a> MappedLocation<'a> {
    /// A shortcut constructor.
    pub fn new(path: String) -> Self {
        Self {
            path,
            line: None,
            column: None,
            length: None,
            source_code_line: None,
        }
    }

    /// A shortcut constructor.
    pub fn new_with_location(
        path: String,
        line: usize,
        column: usize,
        length: usize,
        source_code_line: Option<&'a str>,
    ) -> Self {
        Self {
            path,
            line: Some(line),
            column: Some(column),
            length: Some(length),
            source_code_line,
        }
    }

    /// A shortcut constructor from `solc` AST source location.
    pub fn try_from_source_location(
        source_location: &SourceLocation,
        source_code: Option<&'a str>,
    ) -> Self {
        let source_code = match source_code {
            Some(source_code) => source_code,
            None => return Self::new(source_location.file.to_owned()),
        };
        if source_location.start <= 0 || source_location.end <= 0 {
            return Self::new(source_location.file.to_owned());
        }
        let start = source_location.start as usize;
        let end = source_location.end as usize;

        let mut line_start = 0;
        for (line, source_line) in source_code.split_inclusive('\n').enumerate() {
            let line_end = line_start + source_line.len();

            if start < line_end {
                let line = line + 1;
                let column = start - line_start + 1;
                let length = end - start;
                return Self::new_with_location(
                    source_location.file.to_owned(),
                    line,
                    column,
                    length,
                    Some(source_line.trim_end_matches(['\r', '\n'])),
                );
            }

            line_start = line_end;
        }

        Self::new(source_location.file.to_owned())
    }
}

impl std::fmt::Display for MappedLocation<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let mut path = self.path.clone();
        if let Some(line) = self.line {
            path.push(':');
            path.push_str(line.to_string().as_str());
            if let Some(column) = self.column {
                path.push(':');
                path.push_str(column.to_string().as_str());
                if let (Some(source_code_line), Some(length)) = (self.source_code_line, self.length)
                {
                    let line_number_length = line.to_string().len();
                    writeln!(f, "{} --> {path}", " ".repeat(line_number_length))?;
                    writeln!(f, " {} |", " ".repeat(line_number_length))?;
                    writeln!(f, " {line} | {source_code_line}")?;
                    let caret_offset = column.saturating_sub(1);
                    writeln!(
                        f,
                        " {} | {}{}",
                        " ".repeat(line_number_length),
                        " ".repeat(caret_offset),
                        "^".repeat(std::cmp::min(
                            length,
                            source_code_line.len().saturating_sub(caret_offset)
                        ))
                    )?;
                }
            }
        } else {
            writeln!(f, "--> {path}")?;
        }
        Ok(())
    }
}
