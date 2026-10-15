#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resource(String);

impl Resource {
    pub fn new(s: &str) -> Option<Self> {
        let _ = s;
        todo!()
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
