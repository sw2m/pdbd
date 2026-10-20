// see README.md §"L1 — the transport" and §"License"

mod access;
mod address;
mod channel;
mod dalan;
mod error;
mod exclusion;
mod half;
mod host;
mod lex;
mod nature;
mod option;
mod port;
mod reason;
mod resource;
mod target;
mod text;
mod unlinked;

pub use access::Access;
pub use address::Address;
pub use channel::Channel;
pub use dalan::{Dalan, Item, Width};
pub use error::{Error, Kind, Part};
pub use exclusion::Exclusion;
pub use half::Half;
pub use host::{Host, Name, Zone};
pub use option::{Opt, Options, Unchecked};
pub use port::{Port, Service};
pub use reason::Reason;
pub use resource::Resource;
pub use target::{Gram, Inet, Mode, Mq, Net, Point, Socks, Target, Vsock, Web};
pub use text::Text;
pub use unlinked::Unlinked;

pub(crate) use nature::Nature;
