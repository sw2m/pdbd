use crate::hello::{Offer, Refusal};
use proto::v1::control_service_server::{ControlService, ControlServiceServer};
use proto::v1::{
    CommandEvent, DropRequest, DropResponse, ExecRequest, HelloRequest,
    HelloResponse, ListRequest, ListResponse, ResizeRequest, ResizeResponse,
    ShellRequest, SocatRequest, SocatResponse,
};
use std::error::Error;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_stream::Stream;
use tonic::codegen::BoxStream;
use tonic::transport::server::Connected;
use tonic::{Request, Response, Status};

pub struct Server {
    offer: Offer,
}

impl Server {
    pub fn new(offer: Offer) -> Self {
        Self { offer }
    }

    pub async fn serve<I, IO, IE>(
        self,
        incoming: I,
    ) -> Result<(), tonic::transport::Error>
    where
        I: Stream<Item = Result<IO, IE>>,
        IO: AsyncRead + AsyncWrite + Connected + Unpin + Send + 'static,
        IE: Into<Box<dyn Error + Send + Sync>>,
    {
        tonic::transport::Server::builder()
            .add_service(ControlServiceServer::new(self))
            .serve_with_incoming(incoming)
            .await
    }
}

impl From<Refusal> for Status {
    fn from(refusal: Refusal) -> Self {
        todo!()
    }
}

#[tonic::async_trait]
impl ControlService for Server {
    type ExecStream = BoxStream<CommandEvent>;
    type ShellStream = BoxStream<CommandEvent>;

    async fn hello(
        &self,
        request: Request<HelloRequest>,
    ) -> Result<Response<HelloResponse>, Status> {
        todo!()
    }

    async fn exec(
        &self,
        _: Request<ExecRequest>,
    ) -> Result<Response<Self::ExecStream>, Status> {
        todo!()
    }

    async fn shell(
        &self,
        _: Request<ShellRequest>,
    ) -> Result<Response<Self::ShellStream>, Status> {
        todo!()
    }

    async fn resize(
        &self,
        _: Request<ResizeRequest>,
    ) -> Result<Response<ResizeResponse>, Status> {
        todo!()
    }

    async fn socat(
        &self,
        _: Request<SocatRequest>,
    ) -> Result<Response<SocatResponse>, Status> {
        todo!()
    }

    async fn drop(
        &self,
        _: Request<DropRequest>,
    ) -> Result<Response<DropResponse>, Status> {
        todo!()
    }

    async fn list(
        &self,
        _: Request<ListRequest>,
    ) -> Result<Response<ListResponse>, Status> {
        todo!()
    }
}
