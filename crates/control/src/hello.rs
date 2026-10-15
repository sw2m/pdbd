use proto::v1::{HelloRequest, HelloResponse};
use std::collections::BTreeSet;
use std::fmt;
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
        Self {
            implementation: implementation.into(),
            wire: WIRE,
            floor: WIRE,
            features: features.into_iter().map(Into::into).collect(),
        }
    }

    pub fn request(&self) -> HelloRequest {
        HelloRequest {
            implementation: self.implementation.clone(),
            wire_version: self.wire,
            features: self.features.iter().cloned().collect(),
        }
    }

    pub fn answer(&self, peer: &HelloRequest) -> Result<Agreement, Refusal> {
        let mut agreed = Agreement::read(
            &peer.implementation,
            peer.wire_version,
            &peer.features,
        )?;
        if agreed.wire < self.floor {
            return Err(self.refuse(agreed.wire));
        }
        agreed.wire = agreed.wire.min(self.wire);
        agreed.features.retain(|f| self.features.contains(f));
        Ok(agreed)
    }

    pub fn response(&self, agreement: &Agreement) -> HelloResponse {
        HelloResponse {
            implementation: self.implementation.clone(),
            wire_version: agreement.wire,
            features: agreement.features.iter().cloned().collect(),
        }
    }

    pub fn accept(&self, peer: &HelloResponse) -> Result<Agreement, Refusal> {
        let agreed = Agreement::read(
            &peer.implementation,
            peer.wire_version,
            &peer.features,
        )?;
        if !(self.floor..=self.wire).contains(&agreed.wire) {
            return Err(self.refuse(agreed.wire));
        }
        if let Some(f) = agreed.features.difference(&self.features).next() {
            return Err(Refusal::Unoffered(f.clone()));
        }
        Ok(agreed)
    }

    fn refuse(&self, wire: u32) -> Refusal {
        Refusal::Version {
            wire,
            range: self.floor..=self.wire,
        }
    }
}

impl Agreement {
    fn read(
        implementation: &str,
        wire: u32,
        features: &[String],
    ) -> Result<Self, Refusal> {
        if implementation.is_empty() {
            return Err(Refusal::Implementation);
        }
        if features.iter().any(String::is_empty) {
            return Err(Refusal::Token);
        }
        Ok(Self {
            implementation: implementation.into(),
            wire,
            features: features.iter().cloned().collect(),
        })
    }
}

impl fmt::Display for Refusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Implementation => write!(f, "empty implementation id"),
            Self::Version { wire, range } => write!(
                f,
                "wire_version {wire} is outside the supported pdbd.v1 revisions {}..={}",
                range.start(),
                range.end()
            ),
            Self::Token => write!(f, "empty feature token"),
            Self::Unoffered(feature) => {
                write!(f, "feature {feature:?} was never offered")
            }
        }
    }
}

impl std::error::Error for Refusal {}

#[cfg(test)]
mod test;
