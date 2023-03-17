# MCloudTT
A cloud-native asynchronous MQTT V5 Broker written in Rust.

![CI](https://img.shields.io/github/actions/workflow/status/McloudTT/mcloudtt/ci.yml?style=flat-square)
[![GitHub tag](https://img.shields.io/github/tag/MCloudTT/mcloudtt?include_prereleases=&sort=semver&color=blue&style=flat-square)](https://github.com/MCloudTT/mcloudtt/releases/)
[![License](https://img.shields.io/badge/License-AGPLv3-blue?style=flat-square)](#license)
[![issues - mcloudtt](https://img.shields.io/github/issues/MCloudTT/mcloudtt?style=flat-square)](https://github.com/MCloudTT/mcloudtt/issues)
![Commits/m](https://img.shields.io/github/commit-activity/m/McloudTT/mcloudtt?style=flat-square)

## Features
- [x] MQTT V5
- [x] Websocket
- [x] TLS
- [x] Authentication via TLS
- [x] BigQuery Logging
- [x] Redis Backend
- [x] Docker
- [x] Kubernetes
- [ ] MQTT V3.1.1(maybe)
- [ ] MQTT V3(not planned)

## Architecture overview
![cluster_overview_dark](https://user-images.githubusercontent.com/60036186/221888422-4178ece7-0134-4fa1-ac89-67237565acf2.png)

## Documentation

<div align="center">

[![view - Documentation](https://img.shields.io/badge/view-Documentation-blue?style=for-the-badge)](/docs/ "Go to project documentation")

</div>

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
| Feature         | Description                                                                                              |
|-----------------|----------------------------------------------------------------------------------------------------------|
| `secure`        | Enabled by default. Enables TLS and authentication via TLS. Disable only if you know what you are doing. |
| `docker`        | Enables the `docker` feature, which is as of now sets the right IP Address for the broker to listen on.  |
| `bq_logging`    | Enables logging to BigQuery. Requires an `sa.key` file                                                   |
| `redis`         | Enables Redis as a backend. For distributed/Kubernetes setups                                            |
| `tokio_console` | Enables monitoring via the tokio console.                                                                |

When deploying in a cluster, you can also use the [BigQuery-Adapter](https://github.com/MCloudTT/bigquery) instead of the broker-feature `bq_logging`.

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
```

## Example Usage

### Using mosquitto_sub to listen on a topic
`mosquitto_sub -p 1883 -t "test" --cafile certs/ca.crt --cert certs/client/client.crt --key certs/client/client.key -d
--insecure -V 5 -q 0`

### Using mosquitto_pub to publish to topic
`mosquitto_pub -p 1883 -t "test" -m "test message" --cafi
le certs/ca.crt --cert certs/client/client.crt --key certs
/client/client.key -d --insecure -V 5 -q 0`
## Google Cloud
The project is meant to be deployed on a Google Cloud Kubernetes cluster (using Autopilot).

### Creating cluster
```bash
cd infra
terraform apply
```

### Deplying to cluster
```bash
gcloud container clusters get-credentials mcloudtt-dev-cluster --region REGION --project PROJECT_ID
kubectl create -f mcloudtt_manifest.yml
```

## License
This project uses the `webpki` and `ring` crates by Brian Smith. For them the following license applies:

- ring https://github.com/briansmith/ring/blob/main/LICENSE
- webpki https://github.com/briansmith/webpki/blob/main/LICENSE

## Credits
[BSchwind's MQTT Broker for the Package En/Decoding](https://github.com/bschwind/mqtt-broker)
