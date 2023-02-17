#![no_std]
#![no_main]
#![feature(c_variadic)]
#![feature(const_mut_refs)]
#![feature(type_alias_impl_trait)]
//! This example demonstrates using the ESP32C3 to send an MQTT message to a broker, when a motion
//! sensor is triggered.(In this case a HC-SR501 PIR sensor)
use embassy_executor::_export::StaticCell;
use embassy_net::tcp::TcpSocket;
use embassy_net::{Config, Ipv4Address, Stack, StackResources};
use esp32c3_hal as hal;

use embassy_executor::Executor;
use embassy_time::{Duration, Timer};
use embedded_svc::wifi::{ClientConfiguration, Configuration, Wifi};
use esp_backtrace as _;
use esp_println::logger::init_logger;
use esp_println::println;
use esp_wifi::initialize;
use esp_wifi::wifi::{WifiController, WifiDevice, WifiEvent, WifiState};
use hal::clock::{ClockControl, CpuClock};
use hal::Rng;
use hal::{embassy, peripherals::Peripherals, prelude::*, timer::TimerGroup, Rtc};

use embedded_io::asynch::Write;
use esp32c3_hal::gpio::{
    Bank0GpioRegisterAccess, Gpio10Signals, GpioPin, Input, InputOutputPinType, PullDown,
    SingleCoreInteruptStatusRegisterAccessBank0,
};
use esp32c3_hal::IO;
use riscv_rt::entry;

use hal::systimer::SystemTimer;

const CONNECT_PACKET: &[u8] = b"\x10\x0e\0\0\x05\0\0\0\0\0\x05esp32";
const PUBLISH_PACKET: &[u8] = b"0\"\0\x08esp32-c3\0Motion sensor triggered";
const PING_REQ: &[u8] = b"\xc0\0";
const SSID: &str = env!("SSID");
const PASSWORD: &str = env!("PASSWORD");

macro_rules! singleton {
    ($val:expr) => {{
        type T = impl Sized;
        static STATIC_CELL: StaticCell<T> = StaticCell::new();
        let (x,) = STATIC_CELL.init(($val,));
        x
    }};
}

static EXECUTOR: StaticCell<Executor> = StaticCell::new();

#[entry]
fn main() -> ! {
    init_logger(log::LevelFilter::Info);
    esp_wifi::init_heap();

    let peripherals = Peripherals::take();

    let system = peripherals.SYSTEM.split();

    let clocks = ClockControl::configure(system.clock_control, CpuClock::Clock160MHz).freeze();

    let mut rtc = Rtc::new(peripherals.RTC_CNTL);

    // Disable watchdog timers
    rtc.swd.disable();

    rtc.rwdt.disable();

    let syst = SystemTimer::new(peripherals.SYSTIMER);
    initialize(syst.alarm0, Rng::new(peripherals.RNG), &clocks).unwrap();

    let (wifi_interface, controller) = esp_wifi::wifi::new();

    let timer_group0 = TimerGroup::new(peripherals.TIMG0, &clocks);
    embassy::init(&clocks, timer_group0.timer0);

    let config = Config::Dhcp(Default::default());

    let seed = 1234; // very random, very secure seed

    // Init network stack
    let stack = &*singleton!(Stack::new(
        wifi_interface,
        config,
        singleton!(StackResources::<3>::new()),
        seed
    ));

    let io = IO::new(peripherals.GPIO, peripherals.IO_MUX);
    let inputpin = io.pins.gpio10.into_pull_down_input();
    let executor = EXECUTOR.init(Executor::new());
    executor.run(|spawner| {
        spawner.spawn(connection(controller)).ok();
        spawner.spawn(net_task(&stack)).ok();
        spawner.spawn(task(&stack, inputpin)).ok();
    });
}

#[embassy_executor::task]
async fn connection(mut controller: WifiController) {
    println!("start connection task");
    println!("Device capabilities: {:?}", controller.get_capabilities());
    loop {
        match esp_wifi::wifi::get_wifi_state() {
            WifiState::StaConnected => {
                // wait until we're no longer connected
                controller.wait_for_event(WifiEvent::StaDisconnected).await;
                Timer::after(Duration::from_millis(5000)).await
            }
            _ => {}
        }
        if !matches!(controller.is_started(), Ok(true)) {
            let client_config = Configuration::Client(ClientConfiguration {
                ssid: SSID.into(),
                password: PASSWORD.into(),
                ..Default::default()
            });
            controller.set_configuration(&client_config).unwrap();
            println!("Starting wifi");
            controller.start().await.unwrap();
            println!("Wifi started!");
        }
        println!("About to connect...");

        match controller.connect().await {
            Ok(_) => println!("Wifi connected!"),
            Err(e) => {
                println!("Failed to connect to wifi: {e:?}");
                Timer::after(Duration::from_millis(5000)).await
            }
        }
    }
}

#[embassy_executor::task]
async fn net_task(stack: &'static Stack<WifiDevice>) {
    stack.run().await
}

#[embassy_executor::task]
async fn task(
    stack: &'static Stack<WifiDevice>,
    inputpin: GpioPin<
        Input<PullDown>,
        Bank0GpioRegisterAccess,
        SingleCoreInteruptStatusRegisterAccessBank0,
        InputOutputPinType,
        Gpio10Signals,
        10,
    >,
) {
    let mut rx_buffer = [0; 4096];
    let mut tx_buffer = [0; 4096];

    loop {
        if stack.is_link_up() {
            break;
        }
        Timer::after(Duration::from_millis(500)).await;
    }

    println!("Waiting to get IP address...");
    loop {
        if let Some(config) = stack.config() {
            println!("Got IP: {}", config.address);
            break;
        }
        Timer::after(Duration::from_millis(500)).await;
    }

    loop {
        Timer::after(Duration::from_millis(1_000)).await;

        let mut socket = TcpSocket::new(&stack, &mut rx_buffer, &mut tx_buffer);

        socket.set_timeout(Some(embassy_net::SmolDuration::from_secs(10)));

        loop {
            println!("Polling motion sensor...");
            loop {
                Timer::after(Duration::from_millis(100)).await;
                println!("Sensor is: {:?}", inputpin.is_high().unwrap());
                if inputpin.is_high().unwrap() {
                    break;
                }
            }
            let remote_endpoint = (Ipv4Address::new(34, 66, 45, 21), 1883);
            println!("connecting...");
            let r = socket.connect(remote_endpoint).await;
            if let Err(e) = r {
                println!("connect error: {:?}", e);
                continue;
            }
            println!("connected!");
            let mut buf = [0; 1024];
            println!("Sending Connect packet");
            let r = socket.write_all(CONNECT_PACKET).await;
            if let Err(e) = r {
                println!("write error: {:?}", e);
                break;
            }
            let n = match socket.read(&mut buf).await {
                Ok(0) => {
                    println!("read EOF");
                    break;
                }
                Ok(n) => n,
                Err(e) => {
                    println!("read error: {:?}", e);
                    break;
                }
            };
            println!("{}", core::str::from_utf8(&buf[..n]).unwrap());
            Timer::after(Duration::from_millis(1000)).await;
            println!("Sending Publish packet");
            let r = socket.write_all(PUBLISH_PACKET).await;
            println!("Published, waiting...");
            Timer::after(Duration::from_secs(30)).await;
        }
        Timer::after(Duration::from_secs(120)).await;
    }
}
