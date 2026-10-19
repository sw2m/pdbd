use super::fixture::plain;
use proptest::prelude::*;
use proptest::test_runner::FileFailurePersistence;
use socat::{
    Address, Host, Inet, Mode, Name, Net, Opt, Options, Port, Resource,
    Service, Target, Text, Unchecked, Vsock, Web, Zone,
};
use std::net::Ipv4Addr;
use std::net::Ipv6Addr;
use std::num::NonZeroU32;

mod address;
mod edit;
mod grammar;
mod spelling;

pub(crate) use address::*;
pub(crate) use edit::*;
pub(crate) use grammar::*;
pub(crate) use spelling::*;

pub(crate) fn config() -> ProptestConfig {
    ProptestConfig {
        failure_persistence: Some(Box::new(
            FileFailurePersistence::WithSource("regressions"),
        )),
        ..ProptestConfig::default()
    }
}
