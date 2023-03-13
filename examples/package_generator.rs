use bytes::{Bytes, BytesMut};
use mqtt_v5_fork::encoder::encode_mqtt;
use mqtt_v5_fork::topic::Topic;

use mqtt_v5_fork::types::{Packet, PublishPacket, QoS};
use std::str::FromStr;

fn main() {
    let mut buf = BytesMut::new();
    let packet = Packet::Publish(PublishPacket {
        is_duplicate: false,
        qos: QoS::AtMostOnce,
        retain: false,
        topic: Topic::from_str("esp32-c3").unwrap(),
        packet_id: None,
        payload_format_indicator: None,
        message_expiry_interval: None,
        topic_alias: None,
        response_topic: None,
        correlation_data: None,
        user_properties: vec![],
        subscription_identifiers: vec![],
        content_type: None,
        payload: Bytes::from("Motion sensor triggered"),
    });
    encode_mqtt(&packet, &mut buf, Default::default());
    println!("{:?}", buf);
}
