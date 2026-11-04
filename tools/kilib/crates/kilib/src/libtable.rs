//! `sym-lib-table` and `fp-lib-table`: library nicknames and where they point.

use std::ops::Range;
use std::path::{Path, PathBuf};

use crate::sexpr::{self, Node, ParseError};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TableKind {
    Symbol,
    Footprint,
}

impl TableKind {
    pub fn file_name(self) -> &'static str {
        match self {
            TableKind::Symbol => "sym-lib-table",
            TableKind::Footprint => "fp-lib-table",
        }
    }

    pub fn root(self) -> &'static str {
        match self {
            TableKind::Symbol => "sym_lib_table",
            TableKind::Footprint => "fp_lib_table",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct LibRow {
    pub name: String,
    /// `KiCad`, `Legacy`, `Eagle`, `Database`, `Table` (a nested table), ...
    pub kind: String,
    pub uri: String,
    pub options: String,
    pub descr: String,
    pub disabled: bool,
    /// Where `(lib ...)` sits in the table's text.
    pub span: Range<usize>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LibTable {
    pub kind: TableKind,
    /// Where it was read from, for messages.
    pub path: PathBuf,
    pub rows: Vec<LibRow>,
    /// The span of the closing parenthesis, where new rows go.
    pub end: usize,
}

#[derive(Debug)]
pub enum TableError {
    Io(PathBuf, std::io::Error),
    Parse(PathBuf, ParseError),
    NotATable(PathBuf, String),
}

impl std::fmt::Display for TableError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TableError::Io(p, e) => write!(f, "{}: {e}", p.display()),
            TableError::Parse(p, e) => write!(f, "{}: {e}", p.display()),
            TableError::NotATable(p, head) => {
                write!(f, "{}: starts with ({head} ...), not a library table", p.display())
            }
        }
    }
}

impl std::error::Error for TableError {}

impl LibTable {
    pub fn read(kind: TableKind, path: &Path) -> Result<Self, TableError> {
        let text = std::fs::read_to_string(path).map_err(|e| TableError::Io(path.to_path_buf(), e))?;
        Self::parse(kind, path, &text)
    }

    /// Reads the table at `path` when there is one.
    pub fn read_optional(kind: TableKind, path: &Path) -> Result<Option<Self>, TableError> {
        match std::fs::read_to_string(path) {
            Ok(text) => Self::parse(kind, path, &text).map(Some),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(TableError::Io(path.to_path_buf(), e)),
        }
    }

    pub fn parse(kind: TableKind, path: &Path, text: &str) -> Result<Self, TableError> {
        let root = sexpr::parse(text).map_err(|e| TableError::Parse(path.to_path_buf(), e))?;
        let head = root.head().unwrap_or_default();
        if head != kind.root() {
            return Err(TableError::NotATable(path.to_path_buf(), head.to_string()));
        }
        let rows = root.children("lib").map(row).collect();
        Ok(LibTable { kind, path: path.to_path_buf(), rows, end: root.span().end - 1 })
    }

    pub fn find(&self, name: &str) -> Option<&LibRow> {
        self.rows.iter().find(|r| r.name == name)
    }
}

fn row(n: &Node) -> LibRow {
    let text = |k: &str| n.value(k).unwrap_or_default().to_string();
    LibRow {
        name: text("name"),
        kind: text("type"),
        uri: text("uri"),
        options: text("options"),
        descr: text("descr"),
        disabled: n.child("disabled").is_some(),
        span: n.span(),
    }
}

impl LibRow {
    /// One row the way KiCad 10 writes it.
    pub fn render(name: &str, kind: &str, uri: &str, descr: &str) -> String {
        format!(
            "(lib (name {}) (type {}) (uri {}) (options \"\") (descr {}))",
            sexpr::quote(name),
            sexpr::quote(kind),
            sexpr::quote(uri),
            sexpr::quote(descr)
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_rows_in_both_spacings() {
        let text = "(sym_lib_table\n  (version 7)\n  (lib (name \"a\")(type \"KiCad\")(uri \"${KIPRJMOD}/a.kicad_sym\")(options \"\")(descr \"\"))\n\t(lib (name \"b\") (type \"KiCad\") (uri \"/b\") (options \"\") (descr \"B\") (disabled))\n)\n";
        let t = LibTable::parse(TableKind::Symbol, Path::new("t"), text).unwrap();
        assert_eq!(t.rows.len(), 2);
        assert_eq!(t.find("a").unwrap().uri, "${KIPRJMOD}/a.kicad_sym");
        assert!(t.find("b").unwrap().disabled);
        assert_eq!(&text[t.end..t.end + 1], ")");
        assert!(LibTable::parse(TableKind::Footprint, Path::new("t"), text).is_err());
    }
}
