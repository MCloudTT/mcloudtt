#![forbid(unsafe_code)]
#[cfg(feature = "bq_logging")]
mod bigquery;
mod config;
pub(crate) mod error;
#[cfg(feature = "redis")]
mod redis_client;
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

use mqtt_v5::types::PublishPacket;
use rustls_pemfile::{certs, rsa_private_keys};
use tokio::net::{TcpListener, TcpStream};

use tokio_rustls::rustls::{self, Certificate, PrivateKey};
use tracing::info;
use tracing_subscriber::EnvFilter;

use crate::config::Configuration;
use crate::tcp_handling::Client;
use error::Result;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, Registry};
use tracing_tree::HierarchicalLayer;

use lazy_static::lazy_static;

#[cfg(feature = "docker")]
const LISTENER_ADDR: &str = "0.0.0.0";
#[cfg(not(feature = "docker"))]
const LISTENER_ADDR: &str = "127.0.0.1";

// Load config
lazy_static! {
    static ref SETTINGS: Configuration = Configuration::load().unwrap();
}

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
    main_loop().await
}
async fn main_loop() -> Result {
    let settings = &SETTINGS;

    let topics = Arc::new(Mutex::new(Topics::default()));

    let (redis_sender, redis_receiver) = tokio::sync::mpsc::channel::<PublishPacket>(200);

    // Start redis client
    #[cfg(feature = "redis")]
    {
        let mut redis_client = redis_client::RedisClient::new(
            settings.redis.host.clone(),
            settings.redis.port,
            topics.clone(),
            redis_receiver,
        );

        tokio::spawn(async move {
            redis_client.listen().await;
        });
    }

    // MQTT over TCP
    let listener =
        TcpListener::bind(LISTENER_ADDR.to_owned() + ":" + &settings.ports.tcp.to_string()).await?;
    // MQTT over WebSockets
    let ws_listener =
        TcpListener::bind(LISTENER_ADDR.to_owned() + ":" + &settings.ports.ws.to_string()).await?;

    //TLS
    let certs = load_certs(Path::new(&settings.tls.certfile))?;
    let mut keys = load_keys(Path::new(&settings.tls.keyfile))?;

    let config = rustls::ServerConfig::builder()
        .with_safe_defaults()
        .with_no_client_auth()
        .with_single_cert(certs, keys.remove(0))
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;

    let tls_acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(config));

    // Start loop based on settings for websocket
    if settings.general.websocket {
        loop {
            tokio::select! {
                raw_tcp_stream = listener.accept() => {
                    match raw_tcp_stream {
                        Ok((stream, addr)) => {
                            handle_new_connection(stream, addr, tls_acceptor.clone(), topics.clone(), redis_sender.clone()).await;
                        }
                        Err(e) => {
                            info!("Error accepting TCP connection: {:?}", e);
                        }
                    }
                }
                raw_ws_stream = ws_listener.accept() => {
                    match raw_ws_stream {
                        Ok((stream, addr)) => {
                            handle_new_connection(stream, addr, tls_acceptor.clone(), topics.clone(), redis_sender.clone()).await;
                        }
                        Err(e) => {
                            info!("Error accepting WS connection: {:?}", e);
                        }
                    }
                }
            }
        }
    } else {
        loop {
            tokio::select! {
                raw_tcp_stream = listener.accept() => {
                    match raw_tcp_stream {
                        Ok((stream, addr)) => {
                            handle_new_connection(stream, addr, tls_acceptor.clone(), topics.clone(), redis_sender.clone()).await;
                        }
                        Err(e) => {
                            info!("Error accepting TCP connection: {:?}", e);
                        }
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
    redis_sender: tokio::sync::mpsc::Sender<PublishPacket>,
) {
    info!("Peer connected: {:?}", &addr);
    if let Ok(stream) = tls_acceptor.accept(stream).await {
        let (sender, _receiver) = tokio::sync::mpsc::channel::<Message>(200);
        let mut client = Client::new(sender, topics, redis_sender);
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
