use criterion::{black_box, criterion_group, criterion_main, Criterion};
use paho_mqtt as mqtt;
use std::env;
const HOST: &str = "localhost";
fn try_publish() {}

fn criterion_benchmark(c: &mut Criterion) {
    const CA_CRT: &str = "ca.crt";
    const CLIENT_KEY: &str = "client.key";

    let mut ca_crt = env::current_dir().unwrap();
    ca_crt.push(CA_CRT);

    let mut client_key = env::current_dir().unwrap();
    client_key.push(CLIENT_KEY);
    let cli = mqtt::CreateOptionsBuilder::new()
        .server_uri(HOST)
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
    c.bench_function("fib 20", |b| b.iter(|| try_publish(black_box(data))));
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
