use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Exclusion {
    Openssl,
    Family(String),
}

impl fmt::Display for Exclusion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Openssl => f.write_str("SSL-based: compose it through EXEC"),
            Self::Family(t) => write!(f, "an address-family pin: use {t}"),
        }
    }
}
