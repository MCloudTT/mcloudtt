use bytes::Bytes;
use mqtt_v5::types::PublishPacket;
use redis::Client;
use tracing::error;

use crate::{topics::Topics, Arc};
use std::sync::Mutex;

pub struct RedisClient {
    host: String,
    client: Client,
    topics: Arc<Mutex<Topics>>,
}

impl RedisClient {
    pub fn new(host: String, topics: Arc<Mutex<Topics>>) -> Self {
        Self {
            host: host.clone(),
            client: redis::Client::open(format!("redis://{0}", host)).unwrap(),
            topics,
        }
    }
    pub fn listen(&self) {
        let mut con = self.client.get_connection().unwrap();
        let mut con_sub = con.as_pubsub();

        con_sub.subscribe("sync").unwrap();
        loop {
            let msg = con_sub.get_message().unwrap();
            match msg.get_payload::<Bytes>() {
                Ok(msg) => self.handle_message(msg),
                Err(e) => error!("Error parsing message: {:?}", e),
            }
        }
    }
    pub fn publish(&self, message: PublishPacket) {}

    fn handle_message(&self, message: Bytes) {}
}
