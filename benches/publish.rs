use criterion::async_executor::FuturesExecutor;
use criterion::{black_box, criterion_group, criterion_main, Criterion};

use mosquitto_rs::{Client, QoS};
use std::env;

const HOST: &str = "localhost";

async fn try_publish(msg: &str) {
    const CA_CRT: &str = "certs/ca.crt";
    const CLIENT_KEY: &str = "certs/client/client.key";
    assert!(env::current_dir().unwrap().join(CA_CRT).exists());
    assert!(env::current_dir().unwrap().join(CLIENT_KEY).exists());
    let mut client = Client::with_auto_id().unwrap();
    client
        .configure_tls(
            Some(CA_CRT),
            None::<&str>,
            Some(CLIENT_KEY),
            None::<&str>,
            None,
        )
        .unwrap();
    let rc = client
        .connect(HOST, 1883, std::time::Duration::from_secs(5), None)
        .await
        .unwrap();

    let subscriptions = client.subscriber().unwrap();

    client.subscribe("test/#", QoS::AtMostOnce).await.unwrap();
    println!("subscribed");

    client
        .publish("test/this", msg.as_bytes(), QoS::AtMostOnce, false)
        .await
        .unwrap();
    println!("published");

    subscriptions.recv().await.unwrap();
}

fn criterion_benchmark(c: &mut Criterion) {
    let payload = "Oh hi Mark";
    c.bench_function("fib 20", |b| {
        b.to_async(FuturesExecutor)
            .iter(|| try_publish(black_box(payload)))
    });
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
