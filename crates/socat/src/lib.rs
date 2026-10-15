// see README §"L1 — the transport"; grammar reimplemented from socat(1) and
// websocat's documentation, never socat's source (README §"License")

mod address;
mod error;

pub use address::{Address, Mode, Net, Opt, Socket, Target};
pub use error::{Error, Kind};
