use mqtt_v5::{topic::Topic, types::PublishPacket};
use redis::{Client, Commands, PubSub};
use serde::{Deserialize, Serialize};
use tracing::{error, info};

use crate::{topics::Topics, Arc};
use std::{str::FromStr, sync::Mutex};

pub struct RedisClient {
    host: String,
    client: Client,
    topics: Arc<Mutex<Topics>>,
    receiver: tokio::sync::mpsc::Receiver<PublishPacket>,
}

impl RedisClient {
    pub fn new(
        host: String,
        topics: Arc<Mutex<Topics>>,
        receiver: tokio::sync::mpsc::Receiver<PublishPacket>,
    ) -> Self {
        Self {
            host: host.clone(),
            client: redis::Client::open(format!("redis://{0}", host)).unwrap(),
            topics,
            receiver,
        }
    }
    pub async fn listen(&mut self) {
        let mut con = self.client.get_connection().unwrap();
        let mut con_sub = con.as_pubsub();
        con_sub.subscribe("sync").unwrap();

        loop {
            tokio::select! {
                Some(message) = self.receiver.recv() => {
                    self.publish(message);
                }
                msg = Self::receive_from_redis(&mut con_sub) => {
                    match msg.get_payload::<String>() {
                        Ok(msg) => self.handle_message(msg),
                        Err(e) => error!("Error getting message from redis: {:?}", e),
                    }
                }
            }
        }
    }
    fn publish(&self, message: PublishPacket) {
        let redis_message = RedisMessage {
            topic: message.topic.topic_name().to_owned(),
            payload: message.payload.to_vec(),
            qos: message.qos as u8,
        };
        let mut con = self.client.get_connection().unwrap();
        Commands::publish::<_, _, String>(
            &mut con,
            "sync",
            serde_json::to_value(redis_message).unwrap().to_string(),
        )
        .unwrap();
    }
    fn handle_message(&self, message: String) {
        let redis_message: RedisMessage = serde_json::from_str(&message).unwrap();
        let topic = Topic::from_str(&redis_message.topic).unwrap();
        let publish_packet = PublishPacket::new(topic, redis_message.payload.into());
        match self.topics.lock().unwrap().publish(publish_packet) {
            Ok(_) => info!("Message sent to redis: {0}", message),
            Err(ref e) => error!("Could not send message to redis because of `{0}`", e),
        };
    }
    async fn receive_from_redis(con: &mut PubSub<'_>) -> redis::Msg {
        let msg = con.get_message().unwrap();
        msg
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct RedisMessage {
    topic: String,
    payload: Vec<u8>,
    qos: u8,
}
