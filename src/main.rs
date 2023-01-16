pub(crate) mod error;
mod topics;

use crate::topics::Topics;
use bytes::BytesMut;
use mqtt_v5::decoder::decode_mqtt;
use mqtt_v5::encoder::encode_mqtt;
use mqtt_v5::topic::Topic;
use mqtt_v5::types::properties::{MaximumPacketSize, MaximumQos};
use mqtt_v5::types::{
    ConnectAckPacket, ConnectPacket, ConnectReason, DisconnectPacket, Packet, ProtocolVersion,
    PublishAckPacket, PublishPacket, QoS,
};
use std::io;
use std::net::SocketAddr;
use tokio::io::AsyncReadExt;
use tokio::net::{TcpListener, TcpStream};
use tracing::{debug, info};

const TCP_LISTENER_ADDR: &str = "127.0.0.1:1883";
#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    info!("Starting MCloudTT!");
    let mut topics = Topics::default();
    let listener = TcpListener::bind(TCP_LISTENER_ADDR).await.unwrap();
    while let Ok((stream, addr)) = listener.accept().await {
        info!("Peer connected: {:?}", addr);
        tokio::spawn(handle_raw_tcp_stream(stream, addr));
    }
}
/// read packet from client and decide how to respond
async fn handle_packet(stream: &mut TcpStream) {
    let mut buf = [0; 265];
    let peer = stream.peer_addr().unwrap();
    match stream.read(&mut buf).await {
        Ok(0) => {}
        Ok(n) => {
            info!("Read {:?} bytes", n);
            let packet =
                decode_mqtt(&mut BytesMut::from(buf.as_slice()), ProtocolVersion::V500).unwrap();
            info!("From {:?}: Received packet: {:?}", peer, packet);
            match packet {
                Some(Packet::Connect(p)) => handle_connect_packet(stream, &peer, &p).await,
                Some(Packet::PingRequest) => handle_pingreq_packet(stream).await,
                Some(Packet::Publish(p)) => handle_publish_packet(stream, &peer, &p).await,
                Some(Packet::Disconnect(p)) => handle_disconnect_packet(&peer, &p).await,
                _ => info!("No known packet-type"),
            }
        }
        Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
            debug!("Would block");
        }
        Err(_) => {}
    }
}

// move these function to own file?
async fn handle_connect_packet(stream: &mut TcpStream, peer: &SocketAddr, packet: &ConnectPacket) {
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
    write_to_stream(stream, &ack).await;
}
/// log which client disconneced
async fn handle_disconnect_packet(peer: &SocketAddr, packet: &DisconnectPacket) {
    let reason = packet.reason_code;
    // handle DisconnectWithWill?
    info!("{:?} disconnect with reason-code: {:?}", peer, reason);
}
/// respond to client ping
async fn handle_pingreq_packet(stream: &mut TcpStream) {
    let ping_response = Packet::PingResponse;
    write_to_stream(stream, &ping_response).await;
}
/// process published payload and send PUBACK
async fn handle_publish_packet(stream: &mut TcpStream, peer: &SocketAddr, packet: &PublishPacket) {
    // TODO: process payload
    info!(
        "{:?} published {:?} to {:?}",
        peer, packet.payload, packet.topic
    );
    // packet with a QoS of 0 do get a PUBACK
    if packet.qos != QoS::AtMostOnce {
        let puback = Packet::PublishAck(PublishAckPacket {
            packet_id: packet.packet_id.unwrap(),
            reason_code: mqtt_v5::types::PublishAckReason::Success,
            reason_string: None,
            user_properties: vec![],
        });
        write_to_stream(stream, &puback).await;
    }
}
/// write provided packet to stream
async fn write_to_stream(stream: &mut TcpStream, packet: &Packet) {
    let mut buf = BytesMut::new();
    encode_mqtt(packet, &mut buf, ProtocolVersion::V500);
    let _ = stream.writable().await;
    match stream.try_write(&buf) {
        Ok(e) => {
            info!("Written {} bytes", e);
        }
        Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
            debug!("Would block");
        }
        Err(_) => {}
    }
}
async fn handle_raw_tcp_stream(mut stream: TcpStream, addr: SocketAddr) {
    // wait for new packets from client
    loop {
        match stream.readable().await {
            Ok(_) => handle_packet(&mut stream).await,
            Err(ref e) => info!("ERROR {:?} connection: {:?}", addr, e),
        }
    }
}
