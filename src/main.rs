pub(crate) mod error;
mod tcp_handling;
mod topics;

use crate::topics::{Message, Topics};

use std::sync::{Arc, Mutex};

use tokio::net::TcpListener;

use tracing::info;
use tracing_subscriber::EnvFilter;

use crate::tcp_handling::Client;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, Registry};
use tracing_tree::HierarchicalLayer;

#[cfg(feature = "docker")]
const TCP_LISTENER_ADDR: &str = "0.0.0.0:1883";
#[cfg(not(feature = "docker"))]
const TCP_LISTENER_ADDR: &str = "127.0.0.1:1883";
#[tokio::main]
async fn main() {
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
    let listener = TcpListener::bind(TCP_LISTENER_ADDR).await.unwrap();
    while let Ok((stream, addr)) = listener.accept().await {
        info!("Peer connected: {:?}", addr);
        let (sender, _receiver) = tokio::sync::mpsc::channel::<Message>(200);
        let mut client = Client::new(sender, topics.clone());
        tokio::spawn(async move { client.handle_raw_tcp_stream(stream, addr).await });
    }
}
