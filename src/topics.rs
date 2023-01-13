use std::collections::BTreeMap;

struct Topics<'a> {
    topics: BTreeMap<String, Topic<'a>>,
}
struct Client {
    sender: tokio::sync::mpsc::Sender<Message>,
    receiver: tokio::sync::mpsc::Receiver<Message>,
}
enum Message {
    Publish(String),
    Subscribe(String),
    Unsubscribe(String),
}
struct Topic<'a> {
    name: &'static str,
    subscribers: Vec<&'a Client>,
    messages: Vec<String>,
}
