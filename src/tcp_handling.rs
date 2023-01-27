use crate::error::{MCloudError, Result};
use crate::topics::{Message, Topics};
use bytes::BytesMut;
use std::fmt::Debug;
use std::marker::Unpin;

use mqtt_v5::decoder::decode_mqtt;
use mqtt_v5::encoder::encode_mqtt;
use mqtt_v5::topic::TopicFilter;
use mqtt_v5::types::properties::{MaximumPacketSize, MaximumQos};
use mqtt_v5::types::{
    ConnectAckPacket, ConnectPacket, ConnectReason, DisconnectPacket, Packet, ProtocolVersion,
    PublishAckPacket, PublishPacket, QoS, SubscribeAckPacket, SubscribeAckReason, SubscribePacket,
    UnsubscribeAckPacket, UnsubscribeAckReason, UnsubscribePacket,
};
use std::borrow::Cow;
use std::collections::BTreeMap;
use std::future::Future;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use tokio::io::{AsyncReadExt, AsyncWriteExt, Interest};
use tokio::net::TcpStream;
use tokio::sync::broadcast::Receiver;
use tokio::sync::mpsc::Sender;
use tokio_rustls::server::TlsStream;
use tracing::{debug, info};

#[derive(Debug)]
pub struct Client {
    pub sender: Sender<Message>,
    pub receivers: BTreeMap<String, Receiver<Message>>,
    pub topics: Arc<Mutex<Topics>>,
}

pub trait MCStream: AsyncReadExt + AsyncWriteExt + Unpin + Debug {}
impl MCStream for TlsStream<TcpStream> {}
impl MCStream for TcpStream {}

struct ReceiverFuture<'a> {
    receiver: Vec<(&'a String, &'a Receiver<Message>)>,
}

impl<'a> ReceiverFuture<'a> {
    pub fn new(receiver: Vec<(&'a String, &'a Receiver<Message>)>) -> Self {
        Self { receiver }
    }
}

impl Future for ReceiverFuture<'_> {
    type Output = String;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if let Some(index) = self.receiver.iter().find(|(_, recv)| !recv.is_empty()) {
            Poll::Ready(index.0.to_string())
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
            receivers: BTreeMap::new(),
            topics,
        }
    }

    #[tracing::instrument]
    #[async_backtrace::framed]
    pub async fn handle_raw_tcp_stream(
        &mut self,
        mut stream: impl MCStream,
        addr: SocketAddr,
    ) -> Result {
        loop {
            //TODO: bigger buffer?
            let mut buf = [0; 265];
            tokio::select! {
                    packet = stream.read(&mut buf) => {
                    match packet {
                        Ok(0) => {
                                info!("disconnected unexpectedly");
                                return Err(MCloudError::UnexpectedClientDisconnected("CLIENT".to_string()));
                            },
                        Ok(_) => {
                                info!("RECEIVERS: {:?}", self.receivers);
                                match self.handle_packet(&mut stream, &mut buf).await {
                                    Ok(_) => { },
                                    Err(_) => {
                                        info!("Closing client {0}", &addr);
                                        return Err(MCloudError::ClientError(addr.to_string()));
                                    },
                                };
                            },
                        Err(e) => {
                                info!("Error reading: {0}", e);
                                return Err(MCloudError::ClientError(addr.to_string()));
                            },
                        }
                    }
                    key = ReceiverFuture::new(self.receivers.iter().collect()) => {
                        let message = self.receivers.get_mut(&key).unwrap().recv().await;
                        debug!("Receiver has message: {:?}", message);
                        match message {
                            // We need to only handle publish here
                            Ok(Message::Publish(packet)) => {
                                info!("Subscriber received new message");
                                let send_packet = Packet::Publish(packet);
                                Self::write_to_stream(&mut stream, &send_packet).await?;
                            },
                            _ => continue,
                        };
                    }

            }
        }
    }

    /// Respond to client ping
    #[tracing::instrument]
    #[async_backtrace::framed]
    async fn handle_pingreq_packet(stream: &mut impl MCStream) -> Result {
        let ping_response = Packet::PingResponse;
        Self::write_to_stream(stream, &ping_response).await
    }

    /// Process published payload and send PUBACK
    #[tracing::instrument]
    #[async_backtrace::framed]
    async fn handle_publish_packet(
        &mut self,
        stream: &mut impl MCStream,
        peer: &SocketAddr,
        packet: &PublishPacket,
    ) -> Result {
        // TODO: process payload
        info!(
            "{:?} published {:?} to {:?}",
            peer, packet.payload, packet.topic
        );

        let mut reason_code = mqtt_v5::types::PublishAckReason::UnspecifiedError;

        match self.topics.lock().unwrap().publish(packet.clone()) {
            Ok(_) => {
                reason_code = mqtt_v5::types::PublishAckReason::Success;
                info!("Send message to topic")
            }
            Err(ref e) => info!("Could not send message to topic because of `{0}`", e),
        };

        if packet.qos != QoS::AtMostOnce {
            let puback = Packet::PublishAck(PublishAckPacket {
                packet_id: packet.packet_id.unwrap(),
                reason_code: reason_code,
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
        stream: &mut impl MCStream,
        peer: &SocketAddr,
        packet: &SubscribePacket,
    ) -> Result {
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
                    self.receivers.insert(
                        f.clone(),
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
                TopicFilter::SharedConcrete { .. } | TopicFilter::SharedWildcard { .. } => {
                    sub_ack_packet
                        .reason_codes
                        .push(SubscribeAckReason::SharedSubscriptionsNotSupported)
                }
            }
        }
        let suback = Packet::SubscribeAck(sub_ack_packet);

        // ackknowledge subscription
        Self::write_to_stream(stream, &suback).await
    }

    /// Write provided packet to stream
    #[tracing::instrument]
    #[async_backtrace::framed]
    async fn write_to_stream(stream: &mut impl MCStream, packet: &Packet) -> Result {
        let mut buf = BytesMut::new();
        encode_mqtt(packet, &mut buf, ProtocolVersion::V500);
        //let _ = stream.writable().await;
        match stream.write_all(&buf).await {
            Ok(e) => {
                info!("Written bytes");
                Ok(())
            }
            Err(ref e) => Err(MCloudError::CouldNotWriteToStream(e.to_string())),
        }
    }

    /// Read packet from client and decide how to respond
    #[tracing::instrument]
    #[async_backtrace::framed]
    async fn handle_packet(
        &mut self,
        stream: &mut impl MCStream,
        packet: &mut [u8; 265],
    ) -> Result {
        //TODO: remove peer address
        let peer: SocketAddr = SocketAddrV4::new(Ipv4Addr::new(1, 3, 3, 7), 1337).into();
        let packet = decode_mqtt(
            &mut BytesMut::from(packet.as_slice()),
            ProtocolVersion::V500,
        )
        .unwrap();
        info!("Received packet: {:?}", packet);
        match packet {
            Some(Packet::Connect(p)) => Self::handle_connect_packet(stream, &peer, &p).await,
            Some(Packet::PingRequest) => Self::handle_pingreq_packet(stream).await,
            Some(Packet::Publish(p)) => self.handle_publish_packet(stream, &peer, &p).await,
            Some(Packet::Subscribe(p)) => self.handle_subscribe_packet(stream, &peer, &p).await,
            Some(Packet::Disconnect(p)) => Self::handle_disconnect_packet(&peer, &p).await,
            Some(Packet::Unsubscribe(p)) => self.handle_unsubscribe_packet(stream, &peer, &p).await,
            _ => {
                info!("No known packet-type");
                Err(MCloudError::UnknownPacketType)
            }
        }
    }

    #[tracing::instrument]
    #[async_backtrace::framed]
    async fn handle_connect_packet(
        stream: &mut impl MCStream,
        peer: &SocketAddr,
        packet: &ConnectPacket,
    ) -> Result {
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
    async fn handle_disconnect_packet(peer: &SocketAddr, packet: &DisconnectPacket) -> Result {
        let reason = packet.reason_code;

        // handle DisconnectWithWill?
        info!("{:?} disconnect with reason-code: {:?}", peer, reason);
        Err(MCloudError::ClientDisconnected((&peer).to_string()))
    }

    #[tracing::instrument]
    #[async_backtrace::framed]
    async fn handle_unsubscribe_packet(
        &mut self,
        stream: &mut impl MCStream,
        peer: &SocketAddr,
        packet: &UnsubscribePacket,
    ) -> Result {
        info!("{:?} unsubscribed from {:?}", peer, packet.topic_filters);
        let mut ack = UnsubscribeAckPacket {
            packet_id: packet.packet_id,
            reason_codes: vec![],
            user_properties: vec![],
            reason_string: None,
        };
        packet.topic_filters.iter().for_each(|topic| match topic {
            TopicFilter::Concrete {
                filter,
                level_count: _,
            } => {
                info!("Unsubscribing from {:?}", filter);
                match self.receivers.remove(filter) {
                    Some(_) => ack.reason_codes.push(UnsubscribeAckReason::Success),
                    None => ack
                        .reason_codes
                        .push(UnsubscribeAckReason::NoSubscriptionExisted),
                }
            }
            _ => ack
                .reason_codes
                .push(UnsubscribeAckReason::ImplementationSpecificError),
        });

        let packet = Packet::UnsubscribeAck(ack);
        Self::write_to_stream(stream, &packet).await
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

    fn get_packet(packet: &Packet) -> BytesMut {
        let mut bytes = BytesMut::new();
        encode_mqtt(packet, &mut bytes, Default::default());
        bytes
    }

    #[tokio::test]
    async fn test_has_receiver() {
        let topics = Arc::new(Mutex::new(Topics::default()));
        let mut client = generate_client(topics.clone());
        let (listener, mut writer) = generate_tcp_stream_with_writer("1337".to_string()).await;
        let connect = get_packet(&Packet::Connect(ConnectPacket::default()));
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
        let subscription_packet = get_packet(&Packet::Subscribe(SubscribePacket::new(vec![
            SubscriptionTopic::new_concrete("test"),
        ])));
        writer.write_all(&subscription_packet).await.unwrap();
        writer.flush().await.unwrap();
        sleep(Duration::from_millis(100)).await;
        assert_eq!(1, topics.lock().unwrap().0.len());
    }
    #[tokio::test]
    async fn test_handle_unsubscribe_packet() {
        let topics = Arc::new(Mutex::new(Topics::default()));
        let mut client = generate_client(topics.clone());
        let (listener, mut writer) = generate_tcp_stream_with_writer("1338".to_string()).await;
        let connect = get_packet(&Packet::Connect(ConnectPacket::default()));
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
        let subscription_packet = get_packet(&Packet::Subscribe(SubscribePacket::new(vec![
            SubscriptionTopic::new_concrete("test"),
        ])));
        writer.write_all(&subscription_packet).await.unwrap();
        writer.flush().await.unwrap();
        sleep(Duration::from_millis(20)).await;
        assert_eq!(1, topics.lock().unwrap().0.len());
        let unsubscribe_packet = get_packet(&Packet::Unsubscribe(UnsubscribePacket::new(vec![
            TopicFilter::Concrete {
                filter: "test".to_string(),
                level_count: 1,
            },
        ])));
        writer.write_all(&unsubscribe_packet).await.unwrap();
        writer.flush().await.unwrap();
        sleep(Duration::from_millis(100)).await;
        assert!(matches!(
            topics
                .lock()
                .unwrap()
                .0
                .get_mut("test")
                .unwrap()
                .sender
                .send(Message::Unsubscribe("Hello".to_string())),
            Err(tokio::sync::broadcast::error::SendError(_))
        ));
    }
}
