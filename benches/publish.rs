use criterion::async_executor::AsyncExecutor;
use criterion::async_executor::FuturesExecutor;
use criterion::{black_box, criterion_group, criterion_main, AsyncBencher, Criterion};
use lazy_static::lazy_static;
use paho_mqtt as mqtt;
use paho_mqtt::AsyncClient;
use std::env;

const HOST: &str = "localhost";

// Load config
lazy_static! {
    static ref CLIENT: AsyncClient = mqtt::CreateOptionsBuilder::new()
        .server_uri(HOST)
        .client_id("demo_client")
        .max_buffered_messages(100)
        .create_client()
        .unwrap();
}

async fn try_publish(msg: mqtt::Message) {
    CLIENT.publish(msg).await.unwrap();
}

async fn criterion_benchmark(c: &mut Criterion) {
    const CA_CRT: &str = "ca.crt";
    const CLIENT_KEY: &str = "client.key";

    let mut ca_crt = env::current_dir().unwrap();
    ca_crt.push(CA_CRT);

    let mut client_key = env::current_dir().unwrap();
    client_key.push(CLIENT_KEY);

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

    CLIENT.connect(conn_opts).await.unwrap();
    let data = mqtt::MessageBuilder::new()
        .topic("Thisisatesttopic")
        .payload("Oh hi Mark")
        .qos(0)
        .finalize();
    c.bench_function("fib 20", |b| {
        b.to_async(FuturesExecutor)
            .iter(|| try_publish(black_box(data.clone())))
    });
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
