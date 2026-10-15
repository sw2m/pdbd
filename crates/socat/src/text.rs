#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Text(String);

impl Text {
    pub fn new(s: impl Into<String>) -> Option<Self> {
        let _ = s.into();
        todo!()
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}
