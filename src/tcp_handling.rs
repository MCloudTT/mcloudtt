#[cfg(feature = "bq_logging")]
use crate::bigquery::log_in_bq;
use crate::error::{MCloudError, Result};
use crate::topics::{Message, Topics};
use bytes::BytesMut;
use mqtt_v5::decoder::decode_mqtt;
use mqtt_v5::encoder::encode_mqtt;
use mqtt_v5::topic::TopicFilter;
use mqtt_v5::types::properties::{MaximumPacketSize, MaximumQos, ServerKeepAlive};
use mqtt_v5::types::{
    ConnectAckPacket, ConnectPacket, ConnectReason, DisconnectPacket, DisconnectReason, FinalWill,
    Packet, ProtocolVersion, PublishAckPacket, PublishPacket, QoS, SubscribeAckPacket,
    SubscribeAckReason, SubscribePacket, UnsubscribeAckPacket, UnsubscribeAckReason,
    UnsubscribePacket,
};
use std::borrow::Cow;
use std::collections::BTreeMap;
use std::fmt::Debug;
use std::future::Future;
use std::marker::Unpin;
use std::net::SocketAddr;
use std::pin::Pin;
use std::str;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
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
    pub will: Option<FinalWill>,
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
            will: None,
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
            // TODO: bigger buffer?
            let mut buf = [0; 1024];

            tokio::select! {
                packet = stream.read(&mut buf) => {
                    match packet {
                        Ok(0) => {
                            info!("disconnected unexpectedly");
                            return Err(MCloudError::UnexpectedClientDisconnected(addr.to_string()));
                        },
                        Ok(_) => {
                            info!("RECEIVERS: {:?}", self.receivers);
                            match self.handle_packet(&mut stream, &mut buf, &addr).await {
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
                _ = tokio::time::sleep(Duration::from_secs(10)) => {
                    info!("Will delay interval has passed");
                    self.publish_will(&mut stream, &addr).await?;
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
        // TODO: process payload?
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
        match packet.qos {
            QoS::AtMostOnce => {}
            QoS::AtLeastOnce => {
                let puback = Packet::PublishAck(PublishAckPacket {
                    packet_id: packet.packet_id.unwrap(),
                    reason_code,
                    reason_string: None,
                    user_properties: vec![],
                });
                Self::write_to_stream(stream, &puback).await?;
            }
            QoS::ExactlyOnce => {}
        }
        #[cfg(feature = "bq_logging")]
        log_in_bq(
            packet.topic.topic_name().to_string(),
            str::from_utf8(&packet.payload).unwrap().to_string(),
        )
        .await;
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
        packet: &mut [u8; 1024],
        peer: &SocketAddr,
    ) -> Result {
        let packet = decode_mqtt(
            &mut BytesMut::from(packet.as_slice()),
            ProtocolVersion::V500,
        )
        .unwrap();
        info!("Received packet: {:?}", packet);
        match packet {
            Some(Packet::Connect(p)) => self.handle_connect_packet(stream, &peer, &p).await,
            Some(Packet::PingRequest) => Self::handle_pingreq_packet(stream).await,
            Some(Packet::Publish(p)) => self.handle_publish_packet(stream, &peer, &p).await,
            Some(Packet::Subscribe(p)) => self.handle_subscribe_packet(stream, &peer, &p).await,
            Some(Packet::Disconnect(p)) => self.handle_disconnect_packet(stream, &peer, &p).await,
            Some(Packet::Unsubscribe(p)) => self.handle_unsubscribe_packet(stream, &peer, &p).await,
            Some(Packet::PublishAck(p)) => Ok(()),
            _ => {
                info!("No known packet-type");
                Err(MCloudError::UnknownPacketType)
            }
        }
    }

    #[tracing::instrument]
    #[async_backtrace::framed]
    async fn handle_connect_packet(
        &mut self,
        stream: &mut impl MCStream,
        peer: &SocketAddr,
        packet: &ConnectPacket,
    ) -> Result {
        info!(
            "Connection request from peer {:?} with:\nname: {:?}\nversion: {:?}",
            peer, packet.client_id, packet.protocol_version
        );
        if let Some(will) = &packet.will {
            self.will = Some(will.clone());
        }

        // TODO: check if versions match
        let ack = Packet::ConnectAck(ConnectAckPacket {
            session_present: false,
            reason_code: ConnectReason::Success,
            session_expiry_interval: None,
            receive_maximum: None,
            // temp qos on 1
            maximum_qos: Some(MaximumQos(QoS::AtLeastOnce)),
            retain_available: None,
            maximum_packet_size: Some(MaximumPacketSize(1024)),
            // TODO: assign unique client_identifier
            assigned_client_identifier: None,
            topic_alias_maximum: None,
            reason_string: None,
            user_properties: vec![],
            wildcard_subscription_available: None,
            subscription_identifiers_available: None,
            shared_subscription_available: None,
            server_keep_alive: Some(ServerKeepAlive(10)),
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
        &mut self,
        stream: &mut impl MCStream,
        peer: &SocketAddr,
        packet: &DisconnectPacket,
    ) -> Result {
        let reason = packet.reason_code;
        info!("{:?} disconnect with reason-code: {:?}", peer, reason);
        if reason == DisconnectReason::DisconnectWithWillMessage {
            self.publish_will(stream, peer).await?;
        }
        Err(MCloudError::ClientDisconnected((&peer).to_string()))
    }

    /// Publish the stored will message
    #[tracing::instrument]
    #[async_backtrace::framed]
    async fn publish_will(&mut self, stream: &mut impl MCStream, peer: &SocketAddr) -> Result {
        if self.will.is_none() {
            return Ok(());
        }
        let will_packet = PublishPacket::from(self.will.clone().unwrap());
        self.handle_publish_packet(stream, peer, &will_packet)
            .await?;
        debug!("Will message {:?} sent", will_packet);
        Ok(())
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
    use bytes::Bytes;
    use mqtt_v5::topic::Topic;
    use mqtt_v5::types::SubscriptionTopic;
    use std::str::FromStr;
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

    #[tokio::test]
    async fn test_handle_publish_packet() {
        let topics = Arc::new(Mutex::new(Topics::default()));
        let mut receiver = topics
            .lock()
            .unwrap()
            .subscribe(Cow::Owned("test".to_string()))
            .unwrap();
        let mut client = generate_client(topics.clone());
        let (listener, mut writer) = generate_tcp_stream_with_writer("1339".to_string()).await;
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
        let publish_packet = get_packet(&Packet::Publish(PublishPacket::new(
            Topic::from_str("test").unwrap(),
            Bytes::from("test"),
        )));
        writer.write_all(&publish_packet).await.unwrap();
        writer.flush().await.unwrap();
        sleep(Duration::from_millis(100)).await;

        // Receiver should have one message
        assert_eq!(1, receiver.len());

        // Receiver should have the message "test"
        let msg = receiver.recv().await.unwrap();
        match msg {
            Message::Publish(msg) => assert_eq!("test", str::from_utf8(&msg.payload).unwrap()),
            _ => panic!("Wrong message type"),
        }
    }
}
