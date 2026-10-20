use crate::{Half, Reason};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Unlinked {
    half: Option<Half>,
    reason: Reason,
}

impl Unlinked {
    pub fn new(half: Option<Half>, reason: Reason) -> Self {
        Self { half, reason }
    }

    pub fn half(&self) -> Option<Half> {
        self.half
    }

    pub fn reason(&self) -> Reason {
        self.reason
    }
}
