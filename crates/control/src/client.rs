use crate::hello::{Agreement, Offer, Refusal};
use proto::v1::control_service_client::ControlServiceClient;
use std::fmt;
use tonic::transport::Channel;
use tonic::Status;

#[derive(Debug)]
pub enum Error {
    Status(Status),
    Refusal(Refusal),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Status(s) => write!(f, "L7 (control) RPC failed: {s}"),
            Self::Refusal(r) => write!(f, "L7 (control) Hello refused: {r}"),
        }
    }
}

impl std::error::Error for Error {}

pub struct Client {
    rpc: ControlServiceClient<Channel>,
}

impl Client {
    pub fn new(channel: Channel) -> Self {
        Self {
            rpc: ControlServiceClient::new(channel),
        }
    }

    pub async fn hello(&mut self, offer: &Offer) -> Result<Agreement, Error> {
        let answer = self
            .rpc
            .hello(offer.request())
            .await
            .map_err(Error::Status)?;
        offer.accept(answer.get_ref()).map_err(Error::Refusal)
    }
}
