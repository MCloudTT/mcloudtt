use mqtt_v5::types::properties::{MaximumPacketSize, MaximumQos};
use mqtt_v5::types::{
    ConnectAckPacket, ConnectPacket, ConnectReason, DisconnectPacket, Packet, ProtocolVersion,
    PublishAckPacket, PublishPacket, QoS, SubscribeAckPacket, SubscribeAckReason, SubscribePacket,
};

use crate::error::MCloudError;
use crate::topics::{Message, Topics};
use bytes::BytesMut;
use mqtt_v5::decoder::decode_mqtt;
use mqtt_v5::encoder::encode_mqtt;
use mqtt_v5::topic::TopicFilter;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use tokio::io::AsyncReadExt;
use tokio::net::TcpStream;
use tokio::sync::broadcast::Receiver;
use tokio::sync::mpsc::Sender;
use tracing::info;

#[derive(Debug)]
pub struct Client {
    pub sender: Sender<Message>,
    pub receiver: Option<Receiver<Message>>,
    pub topics: Arc<Mutex<Topics>>,
}
impl Client {
    pub fn new(sender: Sender<Message>, topics: Arc<Mutex<Topics>>) -> Self {
        Self {
            sender,
            receiver: None,
            topics,
        }
    }
    #[tracing::instrument]
    #[async_backtrace::framed]
    pub async fn handle_raw_tcp_stream(&mut self, mut stream: TcpStream, addr: SocketAddr) {
        // wait for new packets from client
        loop {
            match stream.readable().await {
                Ok(_) => match self.handle_packet(&mut stream).await {
                    Ok(_) => continue,
                    Err(_) => {
                        info!("Closing client {0}", &addr);
                        break;
                    }
                },
                Err(ref e) => info!("ERROR {:?} connection: {:?}", addr, e),
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
        stream: &mut TcpStream,
        peer: &SocketAddr,
        packet: &PublishPacket,
    ) -> Result<(), MCloudError> {
        // TODO: process payload
        info!(
            "{:?} published {:?} to {:?}",
            peer, packet.payload, packet.topic
        );
        // Packet with a QoS of 0 do get a PUBACK
        if packet.qos != QoS::AtMostOnce {
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

        // TODO: tell client following features are not supported: SharedSubscriptions, WildcardSubscriptions
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
                    filter: _,
                    level_count: _,
                } => sub_ack_packet
                    .reason_codes
                    .push(SubscribeAckReason::GrantedQoSZero),
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
                Err(MCloudError::UnexpectedClientDisconnected(
                    (&peer).to_string(),
                ))
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
                    Some(Packet::Publish(p)) => {
                        Self::handle_publish_packet(stream, &peer, &p).await
                    }
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
