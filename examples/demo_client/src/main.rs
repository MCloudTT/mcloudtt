use clap::Parser;
use paho_mqtt as mqtt;
use std::env;

#[derive(Parser)]
struct Args {
    host: String,
    topic: String,
    file: String,
    column: usize,
}

#[tokio::main]
async fn main() {
    const CA_CRT: &str = "ca.crt";
    const CLIENT_KEY: &str = "client.key";

    let mut ca_crt = env::current_dir().unwrap();
    ca_crt.push(CA_CRT);

    let mut client_key = env::current_dir().unwrap();
    client_key.push(CLIENT_KEY);

    let args = Args::parse();

    let csv_file = std::fs::File::open(&args.file).unwrap();

    let cli = mqtt::CreateOptionsBuilder::new()
        .server_uri(&args.host)
        .client_id("demo_client")
        .max_buffered_messages(100)
        .create_client()
        .unwrap();

    let ssl_opts = mqtt::SslOptionsBuilder::new()
        .trust_store(ca_crt)
        .unwrap()
        .private_key(client_key)
        .unwrap()
        .verify(false)
        .enable_server_cert_auth(false)
        .disable_default_trust_store(true)
        .finalize();

    let conn_opts = mqtt::ConnectOptionsBuilder::new_v5()
        .ssl_options(ssl_opts)
        .finalize();

    cli.connect(conn_opts).await.unwrap();

    let mut csv_rdr = csv::Reader::from_reader(csv_file);

    for result in csv_rdr.records() {
        let record = result.unwrap();

        let msg = mqtt::MessageBuilder::new()
            .topic(&args.topic)
            .payload(record.get(args.column).unwrap())
            .qos(0)
            .finalize();

        cli.publish(msg).await.unwrap();

        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
    }

    cli.disconnect(None).await.unwrap();
}
