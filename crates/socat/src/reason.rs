#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reason {
    Oneway,
    Queue,
    Echo,
    Inert,
    Interactive,
    Constant,
}

impl Reason {
    pub fn pairs(self) -> bool {
        matches!(self, Self::Oneway | Self::Queue)
    }
}
