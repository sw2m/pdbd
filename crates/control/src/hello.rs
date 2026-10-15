use proto::v1::{HelloRequest, HelloResponse};
use std::collections::BTreeSet;
use std::ops::RangeInclusive;

pub const WIRE: u32 = 1; // the pdbd.v1 revision this implementation speaks

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Offer {
    pub implementation: String,
    pub wire: u32,
    pub floor: u32,
    pub features: BTreeSet<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Agreement {
    pub implementation: String,
    pub wire: u32,
    pub features: BTreeSet<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Refusal {
    Implementation,
    Version {
        wire: u32,
        range: RangeInclusive<u32>,
    },
    Token,
    Unoffered(String),
}

impl Offer {
    pub fn new<I, S>(implementation: &str, features: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        todo!()
    }

    pub fn request(&self) -> HelloRequest {
        todo!()
    }

    pub fn answer(&self, peer: &HelloRequest) -> Result<Agreement, Refusal> {
        todo!()
    }

    pub fn response(&self, agreement: &Agreement) -> HelloResponse {
        todo!()
    }

    pub fn accept(&self, peer: &HelloResponse) -> Result<Agreement, Refusal> {
        todo!()
    }
}

#[cfg(test)]
mod test;
