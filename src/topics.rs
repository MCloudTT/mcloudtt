use crate::error::{MCloudError, Result};
use mqtt_v5::types::PublishPacket;
use std::borrow::Cow;
use std::collections::BTreeMap;
use tokio::sync::broadcast::Sender as BroadcastSender;
use tokio::sync::broadcast::{channel, Receiver};
use tracing::{debug, info};

#[derive(Debug, Default)]
pub struct Topics(pub(crate) BTreeMap<String, Channel>);
impl Topics {
    /// Creates a new topic and returns a receiver for it. Errors if the topic already exists.
    pub(crate) fn add(&mut self, name: Cow<String>) -> Result<Receiver<Message>> {
        let (sender, receiver) = channel(1024);
        if self
            .0
            .insert(name.to_string(), Channel::new(sender))
            .is_none()
        {
            return Ok(receiver);
        }
        Err(MCloudError::TopicAlreadyExists(name.to_string()))
    }
    /// Returns a receiver for the given topic
    pub(crate) fn subscribe(&mut self, name: Cow<String>) -> Result<Receiver<Message>> {
        dbg!("subscribing to topic {}", name.clone());
        if let Some(channel) = self.0.get_mut(&name.to_string()) {
            Ok(channel.sender.subscribe())
        } else {
            info!("Topic {:?} does not exist... Creating it", name);
            Ok(self.add(name)?)
        }
    }
    /// Publishes [packet] to the topic specified in [packet]
    pub(crate) fn publish(&mut self, packet: PublishPacket) -> Result {
        let topic_name = packet.topic.topic_name().to_string();
        if let Some(channel) = self.0.get_mut(&topic_name) {
            channel.sender.send(Message::Publish(packet)).unwrap();
        } else {
            info!("Topic {:?} does not exist... Creating it", topic_name);
            let _ = self.add(Cow::Owned(topic_name.clone()));
            let channel = self.0.get_mut(&topic_name).unwrap();
            let res = channel.sender.send(Message::Publish(packet));
            debug!("{:?}", res);
        }
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq)]
pub enum Message {
    Publish(PublishPacket),
    Subscribe(String),
    Unsubscribe(String),
}
#[derive(Debug)]
pub struct Channel {
    pub sender: BroadcastSender<Message>,
    pub messages: Vec<String>,
}
impl Channel {
    pub fn new(sender: BroadcastSender<Message>) -> Self {
        Self {
            sender,
            messages: vec![],
        }
    }
}
