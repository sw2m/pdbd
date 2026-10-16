use crate::{Error, Kind};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resource(String);

fn invalid(c: char) -> bool {
    c == '#' || c.is_whitespace() || c.is_control()
}

impl Resource {
    pub fn new(s: &str) -> Option<Self> {
        (s.starts_with('/') && !s.contains(invalid)).then(|| Self(s.into()))
    }

    pub(crate) fn parse(path: &str, at: usize) -> Result<Self, Error> {
        Self::new(path).ok_or_else(|| {
            let off = match path.starts_with('/') {
                true => path.find(invalid).unwrap_or(0),
                false => 0,
            };
            Error::new(at + off, Kind::Resource(path.into()))
        })
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for Resource {
    fn default() -> Self {
        Self("/".into())
    }
}
