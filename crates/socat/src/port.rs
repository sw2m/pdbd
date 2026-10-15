#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Service(String);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Port {
    Number(u16),
    Service(Service),
}

impl Service {
    pub fn new(s: &str) -> Option<Self> {
        let _ = s;
        todo!()
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}
