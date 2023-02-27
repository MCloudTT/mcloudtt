use honggfuzz::fuzz;
use mcloudtt::tcp_handling::handle_packet;
fn main() {
    loop {
        fuzz!(|data: &[u8]| { handle_packet });
    }
}
