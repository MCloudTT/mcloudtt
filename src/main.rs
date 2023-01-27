#![forbid(unsafe_code)]
pub(crate) mod error;
mod tcp_handling;
mod topics;

use crate::topics::{Message, Topics};

use std::{
    fs::File,
    io::{self, BufRead, BufReader},
    path::Path,
    sync::{Arc, Mutex},
};

use rustls_pemfile::{certs, rsa_private_keys};
use tokio::net::TcpListener;

use tokio_rustls::rustls::{self, Certificate, PrivateKey};
use tracing::info;
use tracing_subscriber::EnvFilter;

use crate::tcp_handling::Client;
use error::Result;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, Registry};
use tracing_tree::HierarchicalLayer;

#[cfg(feature = "docker")]
const TCP_LISTENER_ADDR: &str = "0.0.0.0:1883";
#[cfg(not(feature = "docker"))]
const TCP_LISTENER_ADDR: &str = "127.0.0.1:1883";
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
    let listener = TcpListener::bind(TCP_LISTENER_ADDR).await?;

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

    // TODO: Handle fallback to non-tls?
    while let Ok((stream, addr)) = listener.accept().await {
        let tls_acceptor = tls_acceptor.clone();
        if let Ok(mut stream) = tls_acceptor.accept(stream).await {
            info!("Peer connected: {:?}", addr);
            let (sender, _receiver) = tokio::sync::mpsc::channel::<Message>(200);
            let mut client = Client::new(sender, topics.clone());
            tokio::spawn(async move { client.handle_raw_tcp_stream(stream, addr).await });
        } else {
            info!("Peer failed to connect: {:?}", addr);
        }
    }
    Ok(())
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
