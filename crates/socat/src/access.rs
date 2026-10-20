use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Access {
    Read,
    Write,
}

impl fmt::Display for Access {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Read => "rdonly",
            Self::Write => "wronly",
        })
    }
}
