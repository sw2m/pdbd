// see README §"L1 — the transport" and §"License"

mod address;
mod error;
mod host;
mod lex;
mod option;
mod target;

pub use address::Address;
pub use error::{Error, Kind, Part};
pub use host::Host;
pub use option::{Opt, Options};
pub use target::{Inet, Mode, Net, Target, Vsock};
