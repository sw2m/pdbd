use super::*;
use hyper_util::rt::TokioIo;
use proto::v1::control_service_client::ControlServiceClient;
use proto::v1::{
    DropRequest, ExecRequest, ListRequest, ResizeRequest, ShellRequest,
    SocatRequest,
};
use std::io;
use tokio::net::TcpListener;
use tokio_stream::wrappers::TcpListenerStream;
use tonic::transport::{Channel, Endpoint};
use tonic::Code;
use tower::service_fn;

const BUFFER: usize = 64 * 1024; // comfortably above one HTTP/2 frame

async fn memory(offer: Offer) -> Channel {
    let (near, far) = tokio::io::duplex(BUFFER);
    tokio::spawn(
        Server::new(offer).serve(tokio_stream::once(Ok::<_, io::Error>(far))),
    );
    let mut near = Some(near);
    Endpoint::from_static("http://memory.invalid")
        .connect_with_connector(service_fn(move |_| {
            let io = near.take().map(TokioIo::new);
            async move {
                io.ok_or_else(|| {
                    io::Error::other("in-memory pipe already dialed")
                })
            }
        }))
        .await
        .expect("dial the in-memory server")
}

async fn loopback(offer: Offer) -> Channel {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(Server::new(offer).serve(TcpListenerStream::new(listener)));
    Endpoint::from_shared(format!("http://{addr}"))
        .unwrap()
        .connect()
        .await
        .expect("dial the loopback server")
}

fn refused(result: Result<Agreement, Error>, code: Code) {
    match result {
        Err(Error::Status(s)) => assert_eq!(s.code(), code, "{s:?}"),
        other => panic!("expected {code:?}, got {other:?}"),
    }
}

// #14
#[tokio::test]
async fn hello_round_trips_in_memory() {
    let channel = memory(Offer::new("pdbd", ["pty", "socat"])).await;
    let agreed = Client::new(channel)
        .hello(&Offer::new("pdb", ["socat", "bind"]))
        .await
        .unwrap();
    assert_eq!(agreed.implementation, "pdbd");
    assert_eq!(agreed.wire, hello::WIRE);
    assert_eq!(agreed.features, ["socat".to_string()].into());
}

// #14
#[tokio::test]
async fn hello_round_trips_over_loopback_tcp() {
    let channel = loopback(Offer::new("pdbd", ["pty"])).await;
    let agreed = Client::new(channel)
        .hello(&Offer::new("pdb", ["pty"]))
        .await
        .unwrap();
    assert_eq!(agreed.implementation, "pdbd");
    assert_eq!(agreed.features, ["pty".to_string()].into());
}

// #14
#[tokio::test]
async fn newer_client_degrades_to_the_server_revision() {
    let channel = memory(Offer::new("pdbd", Vec::<String>::new())).await;
    let mut newer = Offer::new("gopdb", Vec::<String>::new());
    newer.wire = hello::WIRE + 3;
    let agreed = Client::new(channel).hello(&newer).await.unwrap();
    assert_eq!(agreed.wire, hello::WIRE);
}

// #14
#[tokio::test]
async fn server_refuses_a_client_below_its_floor() {
    let mut server = Offer::new("pdbd", Vec::<String>::new());
    server.wire = 3;
    server.floor = 2;
    let channel = memory(server).await;
    let result = Client::new(channel)
        .hello(&Offer::new("cpdb", Vec::<String>::new()))
        .await;
    refused(result, Code::FailedPrecondition);
}

// #14
#[tokio::test]
async fn client_refuses_a_server_below_its_floor() {
    let channel = memory(Offer::new("pdbd", Vec::<String>::new())).await;
    let mut client = Offer::new("gopdb", Vec::<String>::new());
    client.wire = 3;
    client.floor = 2;
    match Client::new(channel).hello(&client).await {
        Err(Error::Refusal(r)) => assert_eq!(
            r,
            Refusal::Version {
                wire: hello::WIRE,
                range: 2..=3
            }
        ),
        other => panic!("expected a version refusal, got {other:?}"),
    }
}

// #14
#[tokio::test]
async fn server_refuses_an_empty_implementation() {
    let channel = memory(Offer::new("pdbd", Vec::<String>::new())).await;
    let result = Client::new(channel)
        .hello(&Offer::new("", Vec::<String>::new()))
        .await;
    refused(result, Code::InvalidArgument);
}

// #14
#[tokio::test]
async fn server_refuses_an_empty_feature_token() {
    let channel = memory(Offer::new("pdbd", ["pty"])).await;
    let result = Client::new(channel)
        .hello(&Offer::new("pdb", ["pty", ""]))
        .await;
    refused(result, Code::InvalidArgument);
}

// #14
#[tokio::test]
async fn unknown_features_are_dropped_over_the_wire() {
    let channel = memory(Offer::new("pdbd", ["pty"])).await;
    let agreed = Client::new(channel)
        .hello(&Offer::new("gopdb", ["x-quic", "x-mux"]))
        .await
        .unwrap();
    assert!(agreed.features.is_empty());
}

// #14
#[tokio::test]
async fn other_rpcs_are_unimplemented() {
    let mut rpc = ControlServiceClient::new(
        memory(Offer::new("pdbd", Vec::<String>::new())).await,
    );
    let codes = [
        rpc.exec(ExecRequest::default())
            .await
            .err()
            .map(|s| s.code()),
        rpc.shell(ShellRequest::default())
            .await
            .err()
            .map(|s| s.code()),
        rpc.resize(ResizeRequest::default())
            .await
            .err()
            .map(|s| s.code()),
        rpc.socat(SocatRequest::default())
            .await
            .err()
            .map(|s| s.code()),
        rpc.drop(DropRequest::default())
            .await
            .err()
            .map(|s| s.code()),
        rpc.list(ListRequest::default())
            .await
            .err()
            .map(|s| s.code()),
    ];
    assert_eq!(codes, [Some(Code::Unimplemented); 6]);
}
