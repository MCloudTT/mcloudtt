use std::collections::BTreeMap;
#[derive(Debug, Default)]
pub struct Topics<'a>(BTreeMap<String, Channel<'a>>);
#[derive(Debug)]
pub struct Client {
    pub sender: tokio::sync::mpsc::Sender<Message>,
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
