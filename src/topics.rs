use crate::error::{MCloudError, Result};
use std::borrow::Cow;
use std::collections::BTreeMap;
use tokio::sync::broadcast::{channel};
use tokio::sync::{broadcast::Sender as BroadcastSender};

#[derive(Debug, Default)]
pub struct Topics(pub(crate) BTreeMap<String, Channel>);
impl Topics {
    pub(crate) fn add(&mut self, name: Cow<String>) -> Result {
        if self
            .0
            .insert(
                name.to_string(),
                Channel::new(channel((usize::MAX >> 1) - 1).0),
            )
            .is_none()
        {
            return Err(MCloudError::TopicAlreadyExists(name.to_string()));
        }
        Ok(())
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
