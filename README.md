# MCloudTT

A cloud-native asynchronous MQTT Broker written in Rust.

## Getting started
Run `gen-keys.sh` to generate required keys and certificates in the certs folder.

After [installing Rust](https://rustup.rs/), run `cargo run --release` to start the broker.

With the `secure` feature enabled, the broker will require TLS and authentication via TLS. 

So to connect to the broker, you will need to provide a client certificate and key. The broker will also require a CA 
certificate to verify the client certificate.

## Docker
To build the docker image, run:
```bash
cargo build --release --target x86_64-unknown-linux-musl --features docker
docker build -t mcloudtt .
```

## Feature Guide
| Feature      | Description                                                                                              |
|--------------|----------------------------------------------------------------------------------------------------------|
| `docker`     | Enables the `docker` feature, which is as of now sets the right IP Address for the broker to listen on.  |
| `bq_logging` | Enables logging to BigQuery. Requires an `sa.key` file                                                   |
| `redis`      | Enables Redis as a backend. For distributed/Kubernetes setups                                            |
| `secure`     | Enabled by default. Enables TLS and authentication via TLS. Disable only if you know what you are doing. |

## Configuration
The broker can be configured via a `config.toml` file. The default configuration is as follows:
```toml
[general]
websocket = true
timeout = 10

[tls]
certfile = "certs/broker/broker.crt"
keyfile = "certs/broker/broker.key"

[ports]
tcp = 1883
ws = 8080

[bigquery]
project_id = "azubi-knowhow-building"
dataset_id = "mcloudttbq"
table_id = "topic-log"
credentials_path = "sa.key"

[redis]
host = "redis"
port = 6379

## Example Usage

### Using mosquitto_sub to listen on a topic
`mosquitto_sub -p 1883 -t "test" --cafile certs/ca.crt --cert certs/client/client.crt --key certs/client/client.key -d
--insecure -V 5 -q 0`

### Using mosquitto_pub to publish to topic
`mosquitto_pub -p 1883 -t "test" -m "test message" --cafi
le certs/ca.crt --cert certs/client/client.crt --key certs
/client/client.key -d --insecure -V 5 -q 0`

## Credits
[BSchwind's MQTT Broker for the Package En/Decoding](https://github.com/bschwind/mqtt-broker)
