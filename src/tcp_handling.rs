use crate::error::MCloudError;
use crate::topics::{Message, Topics};
use bytes::BytesMut;

use mqtt_v5::decoder::decode_mqtt;
use mqtt_v5::encoder::encode_mqtt;
use mqtt_v5::topic::TopicFilter;
use mqtt_v5::types::properties::{MaximumPacketSize, MaximumQos};
use mqtt_v5::types::{
    ConnectAckPacket, ConnectPacket, ConnectReason, DisconnectPacket, Packet, ProtocolVersion,
    PublishAckPacket, PublishPacket, QoS, SubscribeAckPacket, SubscribeAckReason, SubscribePacket,
};
use std::borrow::Cow;
use std::future::Future;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use tokio::io::{AsyncReadExt, Interest};
use tokio::net::TcpStream;
use tokio::sync::broadcast::Receiver;
use tokio::sync::mpsc::Sender;
use tracing::{debug, info};

#[derive(Debug)]
pub struct Client {
    pub sender: Sender<Message>,
    pub receivers: Vec<Receiver<Message>>,
    pub topics: Arc<Mutex<Topics>>,
}

struct ReceiverFuture<'a> {
    receiver: &'a Vec<Receiver<Message>>,
}

impl<'a> ReceiverFuture<'a> {
    pub fn new(receiver: &'a Vec<Receiver<Message>>) -> Self {
        Self { receiver }
    }
}

impl Future for ReceiverFuture<'_> {
    type Output = usize;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if let Some(index) = self
            .receiver
            .iter()
            .enumerate()
            .find(|(_, recv)| !recv.is_empty())
        {
            Poll::Ready(index.0)
        } else {
            cx.waker().wake_by_ref();
            Poll::Pending
        }
    }
}
impl Client {
    pub fn new(sender: Sender<Message>, topics: Arc<Mutex<Topics>>) -> Self {
        Self {
            sender,
            receivers: vec![],
            topics,
        }
    }

    #[tracing::instrument]
    #[async_backtrace::framed]
    pub async fn handle_raw_tcp_stream(
        &mut self,
        mut stream: TcpStream,
        addr: SocketAddr,
    ) -> Result<(), MCloudError> {
        loop {
            tokio::select! {
                _ = stream.ready(Interest::READABLE) => {
                    match self.handle_packet(&mut stream).await {
                        Ok(_) => { },
                        Err(_) => {
                            info!("Closing client {0}", &addr);
                            return Err(MCloudError::ClientError((&addr).to_string()));
                        },
                    };
                }
                index = ReceiverFuture::new(&self.receivers)=> {
                    let message = self.receivers[index].recv().await;
                    debug!("Receiver has message: {:?}", message);
                    match message {
                        Ok(Message::Publish(packet)) => {
                            info!("Subscriber received new message");
                            let send_packet = Packet::Publish(packet);
                            Self::write_to_stream(&mut stream, &send_packet).await
                        },
                        Ok(Message::Subscribe(_)) => continue,
                        Ok(Message::Unsubscribe(_)) => continue,
                        _ => continue,
                    };
                }
            }
        }
    }

    /// Respond to client ping
    #[tracing::instrument]
    #[async_backtrace::framed]
    async fn handle_pingreq_packet(stream: &mut TcpStream) -> Result<(), MCloudError> {
        let ping_response = Packet::PingResponse;
        Self::write_to_stream(stream, &ping_response).await
    }

    /// Process published payload and send PUBACK
    #[tracing::instrument]
    #[async_backtrace::framed]
    async fn handle_publish_packet(
        &mut self,
        stream: &mut TcpStream,
        peer: &SocketAddr,
        packet: &PublishPacket,
    ) -> Result<(), MCloudError> {
        // TODO: process payload
        info!(
            "{:?} published {:?} to {:?}",
            peer, packet.payload, packet.topic
        );

        if packet.qos != QoS::AtMostOnce {
            match self.topics.lock().unwrap().publish(packet.clone()) {
                Ok(_) => info!("Send message to topic"),
                Err(ref e) => info!("Could not send message to topic because of `{0}`", e),
            };
            let puback = Packet::PublishAck(PublishAckPacket {
                packet_id: packet.packet_id.unwrap(),
                reason_code: mqtt_v5::types::PublishAckReason::Success,
                reason_string: None,
                user_properties: vec![],
            });
            return Self::write_to_stream(stream, &puback).await;
        }
        Ok(())
    }

    #[tracing::instrument]
    #[async_backtrace::framed]
    async fn handle_subscribe_packet(
        &mut self,
        stream: &mut TcpStream,
        peer: &SocketAddr,
        packet: &SubscribePacket,
    ) -> Result<(), MCloudError> {
        info!("{:?} subscribed to {:?}", peer, packet.subscription_topics);

        // TODO: tell client following features are not supported: SharedSubscriptions,
        // WildcardSubscriptions
        let mut sub_ack_packet = SubscribeAckPacket {
            packet_id: packet.packet_id,
            reason_string: None,
            user_properties: vec![],
            reason_codes: vec![],
        };

        // Handle unsupported features
        for sub_topic in &packet.subscription_topics {
            let topic_filter = sub_topic.topic_filter.clone();
            match topic_filter {
                TopicFilter::Concrete {
                    filter: f,
                    level_count: _,
                } => {
                    sub_ack_packet
                        .reason_codes
                        .push(SubscribeAckReason::GrantedQoSZero);
                    self.receivers.push(
                        self.topics
                            .lock()
                            .unwrap()
                            .subscribe(Cow::Owned(f.clone()))
                            .unwrap(),
                    );
                    info!("Client {:?} subscribed to {:?}", peer, &f);
                }
                TopicFilter::Wildcard {
                    filter: _,
                    level_count: _,
                } => sub_ack_packet
                    .reason_codes
                    .push(SubscribeAckReason::WildcardSubscriptionsNotSupported),
                TopicFilter::SharedConcrete {
                    group_name: _,
                    filter: _,
                    level_count: _,
                } => sub_ack_packet
                    .reason_codes
                    .push(SubscribeAckReason::SharedSubscriptionsNotSupported),
                TopicFilter::SharedWildcard {
                    group_name: _,
                    filter: _,
                    level_count: _,
                } => sub_ack_packet
                    .reason_codes
                    .push(SubscribeAckReason::SharedSubscriptionsNotSupported),
            }
        }
        let suback = Packet::SubscribeAck(sub_ack_packet);

        // ackknowledge subscription
        Self::write_to_stream(stream, &suback).await
    }

    /// Write provided packet to stream
    #[tracing::instrument]
    #[async_backtrace::framed]
    async fn write_to_stream(stream: &mut TcpStream, packet: &Packet) -> Result<(), MCloudError> {
        let mut buf = BytesMut::new();
        encode_mqtt(packet, &mut buf, ProtocolVersion::V500);
        let _ = stream.writable().await;
        match stream.try_write(&buf) {
            Ok(e) => {
                info!("Written {} bytes", e);
                Ok(())
            }
            Err(ref e) => Err(MCloudError::CouldNotWriteToStream(e.to_string())),
        }
    }

    /// Read packet from client and decide how to respond
    #[tracing::instrument]
    #[async_backtrace::framed]
    async fn handle_packet(&mut self, stream: &mut TcpStream) -> Result<(), MCloudError> {
        let mut buf = [0; 265];
        let peer = stream.peer_addr().unwrap();
        match stream.read(&mut buf).await {
            Ok(0) => {
                info!("{0} disconnected unexpectedly", &peer);
                Err(MCloudError::UnexpectedClientDisconnected(peer.to_string()))
            }
            Ok(n) => {
                info!("Read {:?} bytes", n);
                let packet =
                    decode_mqtt(&mut BytesMut::from(buf.as_slice()), ProtocolVersion::V500)
                        .unwrap();
                info!("From {:?}: Received packet: {:?}", peer, packet);
                match packet {
                    Some(Packet::Connect(p)) => {
                        Self::handle_connect_packet(stream, &peer, &p).await
                    }
                    Some(Packet::PingRequest) => Self::handle_pingreq_packet(stream).await,
                    Some(Packet::Publish(p)) => self.handle_publish_packet(stream, &peer, &p).await,
                    Some(Packet::Subscribe(p)) => {
                        self.handle_subscribe_packet(stream, &peer, &p).await
                    }
                    Some(Packet::Disconnect(p)) => Self::handle_disconnect_packet(&peer, &p).await,
                    _ => {
                        info!("No known packet-type");
                        Err(MCloudError::UnknownPacketType)
                    }
                }
            }
            Err(ref e) => Err(MCloudError::ClientError(e.kind().to_string())),
        }
    }

    #[tracing::instrument]
    #[async_backtrace::framed]
    async fn handle_connect_packet(
        stream: &mut TcpStream,
        peer: &SocketAddr,
        packet: &ConnectPacket,
    ) -> Result<(), MCloudError> {
        info!(
            "Connection request from peer {:?} with:\nname: {:?}\nversion: {:?}",
            peer, packet.client_id, packet.protocol_version
        );

        // TODO: check if versions match
        let ack = Packet::ConnectAck(ConnectAckPacket {
            session_present: false,
            reason_code: ConnectReason::Success,
            session_expiry_interval: None,
            receive_maximum: None,
            // temp qos on 1
            maximum_qos: Some(MaximumQos(QoS::AtMostOnce)),
            retain_available: None,
            // TODO: increase buffer size
            maximum_packet_size: Some(MaximumPacketSize(256)),
            // TODO: assign unique client_identifier
            assigned_client_identifier: None,
            topic_alias_maximum: None,
            reason_string: None,
            user_properties: vec![],
            wildcard_subscription_available: None,
            subscription_identifiers_available: None,
            shared_subscription_available: None,
            server_keep_alive: None,
            response_information: None,
            server_reference: None,
            authentication_method: None,
            authentication_data: None,
        });
        Self::write_to_stream(stream, &ack).await
    }

    /// Log which client disconnected and the reason
    #[tracing::instrument]
    #[async_backtrace::framed]
    async fn handle_disconnect_packet(
        peer: &SocketAddr,
        packet: &DisconnectPacket,
    ) -> Result<(), MCloudError> {
        let reason = packet.reason_code;

        // handle DisconnectWithWill?
        info!("{:?} disconnect with reason-code: {:?}", peer, reason);
        Err(MCloudError::ClientDisconnected((&peer).to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mqtt_v5::types::SubscriptionTopic;
    use std::time::Duration;
    use tokio::io::AsyncWriteExt;
    use tokio::net::TcpListener;
    use tokio::net::TcpStream;
    use tokio::sync::mpsc::channel;
    use tokio::time::sleep;

    async fn generate_tcp_stream_with_writer(port: String) -> (TcpListener, TcpStream) {
        let listener = TcpListener::bind(format!("127.0.0.1:{port}"))
            .await
            .unwrap();
        let writer = TcpStream::connect(format!("127.0.0.1:{port}"))
            .await
            .unwrap();
        (listener, writer)
    }

    fn generate_client(topics: Arc<Mutex<Topics>>) -> Client {
        let (tx, _) = channel(1024);
        Client::new(tx, topics)
    }

    #[tokio::test]
    async fn test_has_receiver() {
        let topics = Arc::new(Mutex::new(Topics::default()));
        let mut client = generate_client(topics.clone());
        let (listener, mut writer) = generate_tcp_stream_with_writer("1337".to_string()).await;
        let mut connect = BytesMut::new();
        encode_mqtt(
            &Packet::Connect(ConnectPacket::default()),
            &mut connect,
            ProtocolVersion::V500,
        );
        let (stream, addr) = listener.accept().await.unwrap();
        tokio::spawn(async move {
            client.handle_raw_tcp_stream(stream, addr).await.unwrap();
        });
        writer.write_all(&connect).await.unwrap();
        writer.flush().await.unwrap();
        sleep(Duration::from_millis(20)).await;
        let mut buf = [0; 1024];
        writer.try_read(&mut buf).unwrap();
        assert!(!buf.is_empty());
        let response_packet =
            decode_mqtt(&mut BytesMut::from(buf.as_slice()), ProtocolVersion::V500)
                .unwrap()
                .unwrap();
        assert!(matches!(response_packet, Packet::ConnectAck(_)));
        let mut subscription_packet = BytesMut::new();
        encode_mqtt(
            &Packet::Subscribe(SubscribePacket::new(vec![SubscriptionTopic::new_concrete(
                "test",
            )])),
            &mut subscription_packet,
            Default::default(),
        );
        writer.write_all(&subscription_packet).await.unwrap();
        writer.flush().await.unwrap();
        sleep(Duration::from_millis(100)).await;
        assert_eq!(1, topics.lock().unwrap().0.len());
    }
}
