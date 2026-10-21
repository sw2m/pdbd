#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Text(String);

impl Text {
    pub fn new(s: impl Into<String>) -> Option<Self> {
        let s = s.into();
        (!s.is_empty()).then_some(Self(s))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}
