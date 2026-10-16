// see README §"L1 — the transport" and §"License"

mod address;
mod error;
#[cfg(test)]
mod fixture;
mod host;
mod lex;
mod option;
mod port;
mod resource;
#[cfg(test)]
mod strategy;
mod target;
mod text;

pub use address::Address;
pub use error::{Error, Kind, Part};
pub use host::{Host, Name, Zone};
pub use option::{Opt, Options, Unchecked};
pub use port::{Port, Service};
pub use resource::Resource;
pub use target::{Inet, Mode, Net, Target, Vsock, Web};
pub use text::Text;
