// see README §"Why the data plane needs no muxer at all" (the bridge
// exception) and §"Concurrency & flow control"; wire layout in #19

pub mod endpoint;
pub mod flow;
pub mod frame;
pub mod mux;

pub use endpoint::{Alias, Bridge, Endpoint, Side, Spec};
pub use flow::Flow;
pub use frame::{Body, Frame, Proto, Reader};
pub use mux::Mux;
