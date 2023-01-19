use crate::error::{MCloudError, Result};
use std::borrow::Cow;
use std::collections::BTreeMap;
use tokio::sync::broadcast::Sender as BroadcastSender;
use tokio::sync::broadcast::{channel, Receiver};
use tracing::info;

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
}
#[derive(Clone, Debug, PartialEq)]
pub enum Message {
    Publish(String),
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

#[cfg(test)]
mod tests {
    use super::*;
    mod topics {
        use super::*;
        #[test]
        fn add() {
            let mut topics = Topics::default();
            let topic_name: Cow<String> = Cow::Owned("test".to_string());
            topics.add(topic_name.clone()).unwrap();
            assert_eq!(topics.0.len(), 1);
            assert_eq!(topics.0.get("test").unwrap().messages.len(), 0);
            assert!(matches!(
                topics.add(topic_name),
                Err(MCloudError::TopicAlreadyExists(_))
            ));
        }
    }
}
