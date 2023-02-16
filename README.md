# MCloudTT

A cloud-native asynchronous MQTT Broker written in Rust.

## Getting started
Run `gen-keys.sh` to generate required keys and certificates in the certs folder.

After [installing Rust](https://rustup.rs/), run `cargo run --release` to start the broker.

## Docker
To build the docker image, run:
```bash
cargo build --release --target x86_64-unknown-linux-musl --features docker
docker build -t mcloudtt .
```

## Feature flags
- `bq_logging` - enables logging to BigQuery. Requires the file `sa.key` to be present in the current directory.
- `docker` - enables the `docker` feature, which is as of now sets the right IP Address for the broker to listen on.
- `redis` - enable communication with a redis server. Needed for message exchange between brokers.

### Using mosquitto_sub to listen on a topic
```bash
mosquitto_sub -p 1883 -t "test" --cafile certs/ca.crt --cert certs/client/client.crt --key certs/client/client.key -d
--insecure -V 5 -q 0
```

### Using mosquitto_pub to publish to topic
```bash
mosquitto_pub -p 1883 -t "test" -m "test message" --cafi
le certs/ca.crt --cert certs/client/client.crt --key certs
/client/client.key -d --insecure -V 5 -q 0
```

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