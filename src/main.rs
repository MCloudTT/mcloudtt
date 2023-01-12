use bytes::BytesMut;
use mqtt_v5::decoder::decode_mqtt;
use mqtt_v5::encoder::encode_mqtt;
use mqtt_v5::types::properties::{MaximumPacketSize, MaximumQos};
use mqtt_v5::types::{ConnectAckPacket, ConnectReason, Packet, ProtocolVersion, QoS};
use std::io;
use tokio::io::AsyncReadExt;
use tokio::net::{TcpListener, TcpStream};
use tracing::{debug, info};
const WEBSOCKET_TCP_LISTENER_ADDR: &str = "127.0.0.1:1883";
#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    info!("Starting MCloudTT!");
    let listener = TcpListener::bind(WEBSOCKET_TCP_LISTENER_ADDR)
        .await
        .unwrap();
    while let Ok((mut stream, addr)) = listener.accept().await {
        info!("Connected {:?}", addr);
        // tokio::spawn(accept_connection(stream));
        tokio::spawn(handle_raw_tcp_stream(stream));
    }
}
async fn read_stream(mut stream: &mut TcpStream) {
    let mut buf = [0; 265];
    let peer = stream.peer_addr().unwrap();
    match stream.read(&mut buf).await {
        Ok(0) => {}
        Ok(n) => {
            // info!("{:?}", buf);
            let packet =
                decode_mqtt(&mut BytesMut::from(buf.as_slice()), ProtocolVersion::V500).unwrap();
            info!("From {:?}: Received packet: {:?}", peer, packet);
            match packet {
                Some(Packet::Connect(p)) => write_to_stream(&mut stream).await,
                _ => info!("No known packet-type"),
            }
        }
        Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
            debug!("Would block");
        }
        Err(e) => {}
    }
}
async fn write_to_stream(stream: &mut TcpStream) {
    let ack = Packet::ConnectAck(ConnectAckPacket {
        session_present: false,
        reason_code: ConnectReason::Success,
        session_expiry_interval: None,
        receive_maximum: None,
        maximum_qos: Some(MaximumQos(QoS::AtMostOnce)),
        retain_available: None,
        maximum_packet_size: Some(MaximumPacketSize(256)),
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
    let mut buf = BytesMut::new();
    encode_mqtt(&ack, &mut buf, ProtocolVersion::V500);
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
async fn handle_raw_tcp_stream(mut stream: TcpStream) {
    loop {
        stream.readable().await;
        read_stream(&mut stream).await;
        // _ = stream.writable() => {
        //     write_to_stream(&mut stream).await;
        // }
    }
}
