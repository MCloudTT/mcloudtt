# ESP32-Client

This example demonstrates how to connect the ESP32-C3 to an MQTT Broker. 
The code was written for an esp32-c3 with a motion sensor(HC-SR501 PIR Sensor) but can easily be adapted for 
other sensors or ESPs.

## Running the example
After [Installing Rust](https://rustup.rs/), you need to install Rust nightly via:
```bash
rustup toolchain install nightly
```

Then, install `espflash` via:
```bash
cargo install espflash
```
And now you can rust the example:
```bash
cargo run
```

## Credits
- [esp-wifi](https://github.com/esp-rs/esp-wifi) for the pure Rust Wi-Fi driver
- [esp-rs](https://github.com/esp-rs/) for basically the whole ecosystem and their help in the matrix chat