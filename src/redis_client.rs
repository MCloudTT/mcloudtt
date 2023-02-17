use mqtt_v5::{topic::Topic, types::PublishPacket};
use rand::distributions::Alphanumeric;
use rand::{thread_rng, Rng};
use redis::{Client, Commands, Connection};
use serde::{Deserialize, Serialize};
use tracing::{error, info};

use crate::{topics::Topics, Arc};
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
    pub async fn listen(&mut self) {
        let mut con = self.get_connection().await;
        let (sender, mut sub_thread_receiver) = tokio::sync::mpsc::channel::<String>(1024);
        tokio::spawn(async move { Self::receive_from_redis(&mut con, sender).await });

        loop {
            tokio::select! {
                Some(message) = self.receiver.recv() => {
                    info!("Message received from mqtt broker and publish to redis");
                    self.publish(message).await;
                }
                Some(redis_message) = sub_thread_receiver.recv() => {
                    info!("Message received from redis and publish to mqtt broker");
                    self.handle_message(redis_message);
                }
            }
        }
    }
    async fn publish(&self, message: PublishPacket) {
        let redis_message = RedisMessage {
            sender_id: self.sender_id.clone(),
            topic: message.topic.topic_name().to_owned(),
            payload: message.payload.to_vec(),
            qos: message.qos as u8,
        };
        let mut con = self.get_connection().await;
        let _ = Commands::publish::<_, _, String>(
            &mut con,
            "sync",
            serde_json::to_value(redis_message).unwrap().to_string(),
        );
    }
    async fn handle_message(&self, message: String) {
        let redis_message: RedisMessage = serde_json::from_str(&message).unwrap();
        if redis_message.sender_id == self.sender_id {
            return;
        }
        let topic = Topic::from_str(&redis_message.topic).unwrap();
        let publish_packet = PublishPacket::new(topic, redis_message.payload.into());
        match self.topics.lock().await.publish(publish_packet) {
            Ok(_) => info!("Message received from redis and publish to topic"),
            Err(ref e) => error!("Could not send message to channel because of `{0}`", e),
        };
    }
    async fn receive_from_redis(con: &mut Connection, sender: tokio::sync::mpsc::Sender<String>) {
        let mut con_sub = con.as_pubsub();
        con_sub.subscribe("sync").unwrap();
        loop {
            tokio::task::block_in_place(|| {
                let msg = con_sub.get_message().unwrap();
                match msg.get_payload::<String>() {
                    Ok(msg) => {
                        info!("Message received from redis: {:?}", &msg);
                        match sender.blocking_send(msg.clone()) {
                            Ok(_) => info!("Message sent to mqtt broker"),
                            Err(e) => error!("Error sending message to mqtt broker: {:?}", e),
                        }
                    }
                    Err(e) => error!("Error getting message from redis: {:?}", e),
                }
            });
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct RedisMessage {
    sender_id: String,
    topic: String,
    payload: Vec<u8>,
    qos: u8,
}
