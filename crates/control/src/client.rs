use crate::hello::{Agreement, Offer, Refusal};
use proto::v1::control_service_client::ControlServiceClient;
use tonic::transport::Channel;
use tonic::Status;

#[derive(Debug)]
pub enum Error {
    Status(Status),
    Refusal(Refusal),
}

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
        todo!()
    }
}
