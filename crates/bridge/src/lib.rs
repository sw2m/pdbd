// see README §"Why the data plane needs no muxer at all" (the bridge
// exception), §"Concurrency & flow control" and §"The bridge frame"

pub mod endpoint;
pub mod flow;
pub mod frame;
pub mod mux;

pub use endpoint::{Alias, Bridge, Endpoint, Side, Spec};
pub use flow::Flow;
pub use frame::{Body, Frame, Proto, Reader};
pub use mux::Mux;
