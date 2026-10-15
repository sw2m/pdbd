// see README §"Control plane" and §"Topology & lifecycle"
pub mod client;
pub mod hello;
pub mod server;

pub use client::{Client, Error};
pub use hello::{Agreement, Offer, Refusal};
pub use server::Server;

#[cfg(test)]
mod test;
