# MCloudTT

# WARNNG: This branch uses a workaround which disables the SNI extension in the TLS handshake.

A cloud-native aynchronous MQTT Broker written in Rust.

## Getting started
Run `gen-keys.sh` to generate required keys and certificates in the certs folder.

### Using mosquitto_sub to listen on a topic
`mosquitto_sub -p 1883 -t "test" --cafile certs/ca.crt --cert certs/client/client.crt --key certs/client/client.key -d
--insecure -V 5 -q 0`

### Using mosquitto_pub to publich to topic
`mosquitto_pub -p 1883 -t "test" -m "test message" --cafi
le certs/ca.crt --cert certs/client/client.crt --key certs
/client/client.key -d --insecure -V 5 -q 0`