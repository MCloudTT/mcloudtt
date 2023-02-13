use mqtt_v5::{topic::Topic, types::PublishPacket};
use redis::{Client, Commands, Connection};
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
        let (sender, mut sub_thread_receiver) = tokio::sync::mpsc::channel::<String>(1024);
        tokio::spawn(async move { Self::receive_from_redis(&mut con, sender).await });

        loop {
            tokio::select! {
                Some(message) = self.receiver.recv() => {
                    info!("Message received from mqtt broker and publish to redis");
                    self.publish(message);
                }
                Some(redis_message) = sub_thread_receiver.recv() => {
                    info!("Message received from redis and publish to mqtt broker");
                    self.handle_message(redis_message);
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
        let _ = Commands::publish::<_, _, String>(
            &mut con,
            "sync",
            serde_json::to_value(redis_message).unwrap().to_string(),
        );
    }
    fn handle_message(&self, message: String) {
        let redis_message: RedisMessage = serde_json::from_str(&message).unwrap();
        let topic = Topic::from_str(&redis_message.topic).unwrap();
        let publish_packet = PublishPacket::new(topic, redis_message.payload.into());
        match self.topics.lock().unwrap().publish(publish_packet) {
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
    topic: String,
    payload: Vec<u8>,
    qos: u8,
}
