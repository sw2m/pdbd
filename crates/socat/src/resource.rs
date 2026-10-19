use crate::{Error, Kind};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resource(String);

impl Resource {
    const PROBE: &'static str = "ws://h"; // https://www.rfc-editor.org/rfc/rfc6455#section-3

    pub fn new(s: &str) -> Option<Self> {
        Self::check(s).ok().map(|()| Self(s.into()))
    }

    pub(crate) fn parse(path: &str, at: usize) -> Result<Self, Error> {
        Self::check(path)
            .map(|()| Self(path.into()))
            .map_err(|off| Error::new(at + off, Kind::Resource(path.into())))
    }

    // https://www.rfc-editor.org/rfc/rfc3986#section-3.3
    // https://www.rfc-editor.org/rfc/rfc3986#section-3.4
    fn check(s: &str) -> Result<(), usize> {
        if !s.starts_with('/') {
            return Err(0);
        }
        let probe = format!("{}{s}", Self::PROBE);
        match fluent_uri::Uri::parse(probe.as_str()) {
            Ok(uri) if uri.fragment().is_none() => Ok(()),
            Ok(_) => Err(s.find('#').unwrap_or(0)),
            Err(e) => Err(e.index() - Self::PROBE.len()),
        }
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
