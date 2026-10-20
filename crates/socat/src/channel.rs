use crate::{Address, Error, Unlinked};
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Channel {
    Single(Address),
    Dual { read: Address, write: Address },
}

impl Channel {
    pub fn parse(_s: &str) -> Result<Self, Error> {
        todo!()
    }

    pub fn link(&self) -> Result<(), Unlinked> {
        todo!()
    }
}

impl fmt::Display for Channel {
    fn fmt(&self, _f: &mut fmt::Formatter<'_>) -> fmt::Result {
        todo!()
    }
}
