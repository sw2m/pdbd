// see README.md §"L1 — the transport" and §"License"

mod address;
mod channel;
mod error;
mod exclusion;
mod half;
mod host;
mod lex;
mod option;
mod port;
mod reason;
mod resource;
mod target;
mod text;
mod unlinked;

pub use address::Address;
pub use channel::Channel;
pub use error::{Error, Kind, Part};
pub use exclusion::Exclusion;
pub use half::Half;
pub use host::{Host, Name, Zone};
pub use option::{Opt, Options, Unchecked};
pub use port::{Port, Service};
pub use reason::Reason;
pub use resource::Resource;
pub use target::{Inet, Mode, Net, Target, Vsock, Web};
pub use text::Text;
pub use unlinked::Unlinked;
