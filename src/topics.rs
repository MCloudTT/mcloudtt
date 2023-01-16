use crate::error::{MCloudError, Result};
use std::borrow::{Borrow, Cow};
use std::collections::BTreeMap;
use tokio::sync::broadcast::Sender as BroadcastSender;
use tokio::sync::mpsc::Sender;
use tracing::error;
#[derive(Debug, Default)]
pub struct Topics<'a>(BTreeMap<String, Channel<'a>>);
impl<'a> Topics<'a> {
    pub(crate) fn add(&'a mut self, name: Cow<String>, channel: Channel<'a>) -> Result {
        if self.0.insert(name.to_string(), channel).is_none() {
            return Err(MCloudError::TopicAlreadyExists(name.to_string()));
        }
        Ok(())
    }
}
#[derive(Debug)]
pub struct Client {
    pub sender: Sender<Message>,
    pub receiver: tokio::sync::mpsc::Receiver<Message>,
}
#[derive(Debug, PartialEq)]
pub enum Message {
    Publish(String),
    Subscribe(String),
    Unsubscribe(String),
}
#[derive(Debug)]
pub struct Channel<'a> {
    pub subscribers: Vec<&'a Client>,
    pub messages: Vec<String>,
}
