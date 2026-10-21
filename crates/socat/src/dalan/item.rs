use crate::Width;
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Item {
    Bytes(Vec<u8>),
    Int(Width, i128),
    Text(String),
    Char(char),
}

impl Item {
    fn escape(c: char, quote: char) -> String {
        match c {
            '\n' => "\\n".into(),
            '\r' => "\\r".into(),
            '\t' => "\\t".into(),
            '\u{c}' => "\\f".into(),
            '\u{8}' => "\\b".into(),
            '\u{7}' => "\\a".into(),
            '\u{1b}' => "\\e".into(),
            '\0' => "\\0".into(),
            '\\' => "\\\\".into(),
            c if c == quote => format!("\\{c}"),
            c => c.to_string(),
        }
    }

    pub(crate) fn unescape(c: char) -> char {
        match c {
            'n' => '\n',
            'r' => '\r',
            't' => '\t',
            'f' => '\u{c}',
            'b' => '\u{8}',
            'a' => '\u{7}',
            'e' => '\u{1b}',
            '0' => '\0',
            c => c,
        }
    }
}

impl fmt::Display for Item {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Bytes(b) => {
                f.write_str("x")?;
                b.iter().try_for_each(|b| write!(f, "{b:02x}"))
            }
            Self::Int(w, n) => write!(f, "{}{n}", w.letter()),
            Self::Text(t) => {
                let body: String =
                    t.chars().map(|c| Self::escape(c, '"')).collect();
                write!(f, "\"{body}\"")
            }
            Self::Char(c) => write!(f, "'{}'", Self::escape(*c, '\'')),
        }
    }
}
