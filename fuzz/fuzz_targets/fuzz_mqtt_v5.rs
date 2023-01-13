extern crate core;

use bytes::BytesMut;
use honggfuzz::fuzz;
use mqtt_v5::decoder::decode_mqtt;
use mqtt_v5::types::ProtocolVersion;

fn main() {
    loop {
        fuzz!(|data: &[u8]| {
            if let Ok(packet) = decode_mqtt(&mut BytesMut::from(data), ProtocolVersion::V500) {
                let _ = packet;
            }
        });
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn crash_1() {
        println!("Fuzz crash 1");
        let bytes = include_bytes!("../hfuzz_workspace/fuzz_mqtt_v5/SIGABRT.PC.7ffff7def64c.STACK.188a85a8a3.CODE.-6.ADDR.0.INSTR.mov____%eax,%ebp.fuzz");
        decode_mqtt(&mut BytesMut::from(bytes.as_slice()), ProtocolVersion::V500).unwrap();
    }
    #[test]
    fn doesthiswork(){
        let data = include_bytes!("../hfuzz_workspace/fuzz_mqtt_v5/SIGABRT.PC.7ffff7def64c.STACK.188a85a8a3.CODE.-6.ADDR.0.INSTR.mov____%eax,%ebp.fuzz");
        if let Ok(packet) = decode_mqtt(&mut BytesMut::from(data.as_slice()), ProtocolVersion::V500) {
            let _ = packet;
        }
    }
}
