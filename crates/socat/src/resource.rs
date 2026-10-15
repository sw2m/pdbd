#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resource(String);

impl Resource {
    pub fn new(s: &str) -> Option<Self> {
        let valid = s.starts_with('/')
            && !s.chars().any(|c| c.is_whitespace() || c.is_control());
        valid.then(|| Self(s.into()))
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
