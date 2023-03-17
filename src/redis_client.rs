use mqtt_v5_fork::{topic::Topic, types::PublishPacket, types::PublishPacketBuilder};
use rand::distributions::Alphanumeric;
use rand::{thread_rng, Rng};
use redis::{Client, Commands, Connection, Msg};
use serde::{Deserialize, Serialize};
use tracing::{error, info};

use crate::error::MCloudError;
use crate::{topics::Topics, Arc};
use std::borrow::Cow;
use std::str::FromStr;
use tokio::sync::Mutex;

pub struct RedisClient {
    client: Client,
    topics: Arc<Mutex<Topics>>,
    receiver: tokio::sync::mpsc::Receiver<PublishPacket>,
    sender_id: String,
}

impl RedisClient {
    pub fn new(
        host: String,
        port: u16,
        topics: Arc<Mutex<Topics>>,
        receiver: tokio::sync::mpsc::Receiver<PublishPacket>,
    ) -> Self {
        Self {
            client: redis::Client::open(format!("redis://{0}:{1}", host, port)).unwrap(),
            topics,
            receiver,
            sender_id: thread_rng()
                .sample_iter(&Alphanumeric)
                .take(8)
                .map(char::from)
                .collect(),
        }
    }
    /// Get a connection to redis instance
    async fn get_connection(&self) -> Connection {
        loop {
            match self.client.get_connection() {
                Ok(con) => return con,
                Err(e) => {
                    error!("Error connecting to redis: {0} (retrying in one second)", e);
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                }
            }
        }
    }
    /// Receive messages from redis pubsub channel "sync" and the broker
    pub async fn listen(&mut self) {
        let (sender, mut sub_thread_receiver) = tokio::sync::mpsc::channel::<String>(1024);
        let redis_client_clone = self.client.clone();
        tokio::spawn(async move { Self::receive_from_redis(redis_client_clone, sender).await });

        loop {
            tokio::select! {
                // Receive messages from mqtt broker
                Some(message) = self.receiver.recv() => {
                    info!("Message received from mqtt broker and publish to redis");
                    self.publish(message).await;
                }
                // Receive messages from redis pubsub channel "sync"
                Some(redis_message) = sub_thread_receiver.recv() => {
                    info!("Message received from redis and publish to mqtt broker");
                    self.handle_message(redis_message).await;
                }
            }
        }
    }
    /// Publish message on redis pubsub channel "sync" and save message in redis if it is a retained message
    async fn publish(&self, message: PublishPacket) {
        let redis_message = RedisMessage {
            sender_id: self.sender_id.clone(),
            topic: message.topic.topic_name().to_owned(),
            payload: message.payload.to_vec(),
            qos: message.qos as u8,
            retain: message.retain,
        };
        let mut con = self.get_connection().await;
        let redis_message_str = serde_json::to_value(redis_message).unwrap().to_string();
        // Publish message on redis pubsub channel "sync"
        let _ = redis::cmd("PUBLISH")
            .arg("sync")
            .arg(&redis_message_str)
            .query::<String>(&mut con);
        // Save message in redis if it is a retained message with a key in format "retain:{topic}"
        if message.retain {
            let _ = redis::cmd("SET")
                .arg(format!("retain:{}", &message.topic.topic_name().to_owned()))
                .arg(&redis_message_str)
                .query::<String>(&mut con);
        }
    }
    /// Handle a message received from redis
    async fn handle_message(&self, message: String) {
        let redis_message: RedisMessage = serde_json::from_str(&message).unwrap();
        if redis_message.sender_id == self.sender_id {
            return;
        }
        let publish_packet =
            PublishPacketBuilder::new(redis_message.topic.clone(), redis_message.payload.into())
                .with_retain(redis_message.retain)
                .build()
                .unwrap();
        // Publish message to mqtt broker
        match self.topics.lock().await.publish(publish_packet) {
            Ok(_) => info!("Message received from redis and publish to topic"),
            Err(ref e) => error!("Could not send message to channel because of `{0}`", e),
        };
    }
    /// Receive messages from redis and send them to the mqtt broker
    async fn receive_from_redis(redis_client: Client, sender: tokio::sync::mpsc::Sender<String>) {
        loop {
            // Get a connection to redis instance
            let mut con = match redis_client.get_connection() {
                Ok(con) => con,
                Err(_) => {
                    error!("Error connecting to redis (retrying in one second)");
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                    continue;
                }
            };
            // Subscribe to redis pubsub channel "sync"
            let mut con_sub = con.as_pubsub();
            if let Err(_) = con_sub.subscribe("sync") {
                error!("Error subscribing to redis channel");
                continue;
            }
            // Block until a message is received
            tokio::task::block_in_place(|| {
                if let Ok(msg) = con_sub.get_message() {
                    Self::handle_redis_message(msg, &sender).unwrap_or_else(|e| {
                        error!("Error handling redis message: {0}", e);
                    });
                } else {
                    error!("Error in redis connection occurred");
                }
            });
        }
    }
    /// Handle a message received from redis
    fn handle_redis_message(
        msg: Msg,
        sender: &tokio::sync::mpsc::Sender<String>,
    ) -> Result<(), MCloudError> {
        let message = msg.get_payload::<String>()?;
        sender.blocking_send(message)?;
        Ok(info!("Message sent to mqtt broker"))
    }
    /// Get all retained messages from redis and add them to the topics
    pub async fn get_all_retained_messages(&self) {
        let mut con = self.get_connection().await;
        // Get all keys in redis that start with "retain:"
        // As this operation is performed only once at startup, it is not a problem to use KEYS
        let keys: Vec<String> = redis::cmd("KEYS")
            .arg("retain:*")
            .query::<Vec<String>>(&mut con)
            .unwrap();
        for key in keys {
            // Get the message from redis and add it to topics
            let redis_message_str = redis::cmd("GET").arg(key).query::<String>(&mut con);
            if let Ok(redis_message_str) = redis_message_str {
                if let Ok(Some(redis_message)) =
                    serde_json::from_str::<std::option::Option<RedisMessage>>(&redis_message_str)
                {
                    let publish_packet = PublishPacketBuilder::new(
                        redis_message.topic.clone(),
                        redis_message.payload.into(),
                    )
                    .build()
                    .unwrap();
                    // Add topic to topics
                    let mut topics = self.topics.lock().await;
                    let _ = topics.add(Cow::Owned(redis_message.topic.clone())).unwrap();
                    let _ = topics
                        .0
                        .get_mut(&redis_message.topic.clone())
                        .unwrap()
                        .retained_message = Some(publish_packet);
                }
            }
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct RedisMessage {
    sender_id: String,
    topic: String,
    payload: Vec<u8>,
    qos: u8,
    retain: bool,
}
