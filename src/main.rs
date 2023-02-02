#![forbid(unsafe_code)]
#[cfg(feature = "bq_logging")]
mod bigquery;
pub(crate) mod error;
mod tcp_handling;
mod topics;

use crate::topics::{Message, Topics};

use std::{
    fs::File,
    io::{self, BufReader},
    net::SocketAddr,
    path::Path,
    sync::{Arc, Mutex},
};

use rustls_pemfile::{certs, rsa_private_keys};
use tokio::net::{TcpListener, TcpStream};

use tokio_rustls::rustls::{self, Certificate, PrivateKey};
use tracing::info;
use tracing_subscriber::EnvFilter;

use crate::tcp_handling::Client;
use error::Result;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, Registry};
use tracing_tree::HierarchicalLayer;

#[cfg(feature = "docker")]
const LISTENER_ADDR: &str = "0.0.0.0";
#[cfg(not(feature = "docker"))]
const LISTENER_ADDR: &str = "127.0.0.1";

#[tokio::main]
async fn main() -> Result {
    // Set up tracing_tree
    Registry::default()
        .with(EnvFilter::from_default_env())
        .with(
            HierarchicalLayer::new(2)
                .with_targets(true)
                .with_bracketed_fields(true),
        )
        .init();
    info!("Starting MCloudTT!");
    let topics = Arc::new(Mutex::new(Topics::default()));

    // MQTT over TCP
    let listener = TcpListener::bind(LISTENER_ADDR.to_owned() + ":1883").await?;
    // MQTT over WebSockets
    let ws_listener = TcpListener::bind(LISTENER_ADDR.to_owned() + ":8080").await?;

    println!("Serving at {:?}", listener.local_addr());

    //TLS
    let certs = load_certs(Path::new("certs/broker/broker.crt"))?;
    let mut keys = load_keys(Path::new("certs/broker/broker.key"))?;

    let config = rustls::ServerConfig::builder()
        .with_safe_defaults()
        .with_no_client_auth()
        .with_single_cert(certs, keys.remove(0))
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;

    println!("TLS config: {:?}", config);

    let tls_acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(config));

    loop {
        tokio::select! {
            raw_tcp_stream = listener.accept() => {
                match raw_tcp_stream {
                    Ok((stream, addr)) => {
                        handle_new_connection(stream, addr, tls_acceptor.clone(), topics.clone()).await;
                    }
                    Err(e) => {
                        info!("Error accepting TCP connection: {:?}", e);
                    }
                }
            }
            raw_ws_stream = ws_listener.accept() => {
                match raw_ws_stream {
                    Ok((stream, addr)) => {
                        handle_new_connection(stream, addr, tls_acceptor.clone(), topics.clone()).await;
                    }
                    Err(e) => {
                        info!("Error accepting WS connection: {:?}", e);
                    }
                }
            }
        }
    }
}

async fn handle_new_connection(
    stream: TcpStream,
    addr: SocketAddr,
    tls_acceptor: tokio_rustls::TlsAcceptor,
    topics: Arc<Mutex<Topics>>,
) {
    info!("Peer connected: {:?}", &addr);
    if let Ok(stream) = tls_acceptor.accept(stream).await {
        let (sender, _receiver) = tokio::sync::mpsc::channel::<Message>(200);
        let mut client = Client::new(sender, topics);
        tokio::spawn(async move { client.handle_raw_tcp_stream(stream, addr).await });
    } else {
        info!("Peer failed to connect: {:?}", addr);
    }
}

fn load_certs(path: &Path) -> io::Result<Vec<Certificate>> {
    certs(&mut BufReader::new(File::open(path)?))
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "invalid cert"))
        .map(|mut certs| certs.drain(..).map(Certificate).collect())
}

fn load_keys(path: &Path) -> io::Result<Vec<PrivateKey>> {
    rsa_private_keys(&mut BufReader::new(File::open(path)?))
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "invalid key"))
        .map(|mut keys| keys.drain(..).map(PrivateKey).collect())
}
