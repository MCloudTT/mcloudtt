pub(crate) mod error;
mod tcp_handling;
mod topics;

use crate::topics::{Message, Topics};

use std::borrow::Cow;

use std::sync::{Arc, Mutex};

use tokio::net::TcpListener;

use tokio::sync::mpsc::Receiver;
use tracing::info;
use tracing_subscriber::EnvFilter;

use crate::tcp_handling::handle_raw_tcp_stream;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, Registry};
use tracing_tree::HierarchicalLayer;

#[cfg(feature = "docker")]
const TCP_LISTENER_ADDR: &str = "0.0.0.0:1883";
#[cfg(not(feature = "docker"))]
const TCP_LISTENER_ADDR: &str = "127.0.0.1:1883";
#[tokio::main]
async fn main() {
    // tracing_subscriber::fmt::init();
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
    let mut receivers: Vec<Receiver<Message>> = vec![];
    let listener = TcpListener::bind(TCP_LISTENER_ADDR).await.unwrap();
    while let Ok((stream, addr)) = listener.accept().await {
        info!("Peer connected: {:?}", addr);
        tokio::spawn(handle_raw_tcp_stream(stream, addr));
        // Iterate through all receivers to see if messages were received and if so publish them to
        // the corresponding channels
        let _spawned_threads: Vec<_> = receivers
            .iter_mut()
            .filter_map(|receiver| receiver.try_recv().ok())
            .map(|message| tokio::spawn(handle_message(message, topics.clone())))
            .collect();
    }
}

#[tracing::instrument]
#[async_backtrace::framed]
async fn handle_message(msg: Message, topics: Arc<Mutex<Topics>>) {
    match msg {
        Message::Publish(topic) => {
            if let Some(channel) = topics.lock().unwrap().0.get_mut(&topic) {
                channel.messages.push(topic);
            } else {
                info!("No channel found for topic: {}, creating it", topic);
                topics
                    .lock()
                    .unwrap()
                    .add(Cow::Owned(topic.to_string()))
                    .unwrap();
            }
        }
        _ => {}
    }
}
