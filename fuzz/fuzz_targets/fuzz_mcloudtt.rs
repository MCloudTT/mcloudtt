use honggfuzz::fuzz;
use mcloudtt::tcp_handling::handle_packet;
fn main() {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            loop {
                fuzz!(|data: &[u8]| {
                    let _ = handle_packet(data).await;
                });
            }
        });
}
