use crate::error::{MCloudError, Result};
use mqtt_v5_fork::types::PublishPacket;
use std::borrow::Cow;
use std::collections::BTreeMap;
use tokio::sync::broadcast::{channel, Receiver};
use tokio::sync::broadcast::{Sender as BroadcastSender, Sender};
use tracing::{debug, info, warn};

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
            let _ = send_message(&mut channel.sender, Message::Publish(packet.clone()));
        } else {
            info!("Topic {:?} does not exist... Creating it", topic_name);
            let _ = self.add(Cow::Owned(topic_name.clone()))?;
            let channel = self.0.get_mut(&topic_name).unwrap();
            let _ = send_message(&mut channel.sender, Message::Publish(packet.clone()));
        }
        if packet.retain {
            info!("Retaining message for topic {:?}", &topic_name);
            self.0.get_mut(&topic_name).unwrap().retained_message = Some(packet);
        }
        Ok(())
    }
    /// Check if the given topic has a retained message and if so, returns it
    pub(crate) fn get_retained_message(&self, topic_name: &str) -> Option<PublishPacket> {
        if let Some(channel) = self.0.get(topic_name) {
            channel.retained_message.clone()
        } else {
            None
        }
    }
}
pub(crate) fn send_message(sender: &mut Sender<Message>, message: Message) -> Result {
    match sender.send(message) {
        Ok(_) => debug!("Sent message"),
        Err(_) => {
            warn!("No receivers found for message");
            return Err(MCloudError::NoReceiversFound);
        }
    }
    Ok(())
}
#[derive(Clone, Debug, PartialEq)]
pub enum Message {
    Publish(PublishPacket),
}
#[derive(Debug)]
pub struct Channel {
    pub sender: BroadcastSender<Message>,
    pub retained_message: Option<PublishPacket>,
}
impl Channel {
    pub fn new(sender: BroadcastSender<Message>) -> Self {
        Self {
            sender,
            retained_message: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    mod topics {
        use super::*;
        #[test]
        fn test_add() {
            let mut topics = Topics::default();
            let topic_name: Cow<String> = Cow::Owned("test".to_string());
            topics.add(topic_name.clone()).unwrap();
            assert_eq!(topics.0.len(), 1);
            assert!(topics.0.get("test").unwrap().retained_message.is_none());
            assert!(matches!(
                topics.add(topic_name),
                Err(MCloudError::TopicAlreadyExists(_))
            ));
        }
    }
}
