extern crate alloc;
use crate::{mk_static, solutils::types::GetBalanceResponse};

use embassy_executor::Spawner;
use embassy_net::tcp::Error;
use embassy_net::{tcp::TcpSocket, Config, Ipv4Address, Stack, StackResources};
use embassy_time::{Duration, Timer};
use embedded_io::ReadExactError;
use embedded_io_async::Read;
use embedded_io_async::Write;
use esp_backtrace as _;
use esp_hal::peripherals::Peripherals;
use esp_println::println;
use esp_wifi::wifi::{
    ClientConfiguration, Configuration, WifiController, WifiDevice, WifiEvent, WifiStaDevice,
    WifiState,
};
use log::{error, info};
use serde_json_core::de::from_slice;

#[embassy_executor::task]
pub async fn wifi_task(
    spawner: Spawner,
    peripherals: Peripherals,
    init: esp_wifi::EspWifiInitialization,
) -> ! {
    let wifi = peripherals.WIFI;
    let (wifi_interface, controller) =
        esp_wifi::wifi::new_with_mode(&init, wifi, WifiStaDevice).unwrap();

    let config = Config::dhcpv4(Default::default());

    let seed = 1234; // very random, very secure seed

    // Init network stack
    let stack = &*mk_static!(
        Stack<WifiDevice<'_, WifiStaDevice>>,
        Stack::new(
            wifi_interface,
            config,
            mk_static!(StackResources<3>, StackResources::<3>::new()),
            seed
        )
    );

    spawner.spawn(connection(controller)).ok();
    spawner.spawn(net_task(stack)).ok();

    let mut rx_buffer = [0; 4096];
    let mut tx_buffer = [0; 4096];

    #[allow(unused_labels)]
    'stack: loop {
        if stack.is_link_up() {
            break;
        }
        Timer::after(Duration::from_millis(500)).await;
    }

    info!("Waiting to get IP address...");
    #[allow(unused_labels)]
    'get_ip: loop {
        if let Some(config) = stack.config_v4() {
            info!("Got IP: {}", config.address);
            break;
        }
        Timer::after(Duration::from_millis(500)).await;
    }

    #[allow(unused_labels)]
    'connect: loop {
        Timer::after(Duration::from_millis(1_000)).await;

        let mut socket = TcpSocket::new(stack, &mut rx_buffer, &mut tx_buffer);
        socket.set_timeout(Some(embassy_time::Duration::from_secs(10)));

        // this is example endpoint
        // let remote_endpoint = (Ipv4Address::new(142, 250, 185, 115), 80);
        // this is a solana api devnet rpc node
        // let remote_endpoint = (Ipv4Address::new(67, 209, 54, 90), 443);
        // this is a solana api localnet rpc node
        let remote_endpoint = (Ipv4Address::new(192, 168, 1, 7), 8899);

        info!("connecting to {:?}", remote_endpoint);
        let r = socket.connect(remote_endpoint).await;
        if let Err(e) = r {
            info!("connect error: {:?}", e);
            continue;
        }
        info!("connected!");

        let mut buf = [0; 1024];

        let get_token_account_balance_request = include_str!("../../request/get_balance.http");
        let separator = b"\r\n\r\n";

        'rw: loop {
            Timer::after(Duration::from_millis(1_000)).await;

            let write_result = socket
                .write_all(get_token_account_balance_request.as_bytes())
                .await;

            if let Err(e) = write_result {
                error!("write error: {:?}", e);
                if e != Error::ConnectionReset {
                    break 'rw;
                }
            }

            if let Err(e) = socket.read_exact(&mut buf).await {
                error!("read error: {:?}", e);
                if e != ReadExactError::Other(Error::ConnectionReset) {
                    break 'rw;
                }
            }

            let actual_length = buf.iter().position(|&x| x == 0).unwrap_or(buf.len());
            let trimmed_buf = &buf[..actual_length];
            let body_start = trimmed_buf
                .windows(separator.len())
                .position(|window| window == separator)
                .map_or(0, |pos| pos + separator.len());
            let body_bytes = &trimmed_buf[body_start..];

            #[cfg(debug_assertions)]
            {
                if let Ok(body_str) = core::str::from_utf8(body_bytes) {
                    println!("Body as JSON str: {}", body_str);
                }
            }

            match from_slice::<GetBalanceResponse>(body_bytes) {
                Ok((parsed_body, _)) => {
                    println!("Parsed body: {:#?}", parsed_body);
                }
                Err(e) => {
                    error!("Failed to parse JSON: {:?}", e);
                    break 'rw;
                }
            };
        }
        Timer::after(Duration::from_millis(3_000)).await;
    }
}

#[embassy_executor::task]
async fn connection(mut controller: WifiController<'static>) {
    info!("start connection task");
    info!("Device capabilities: {:?}", controller.get_capabilities());

    loop {
        if esp_wifi::wifi::get_wifi_state() == WifiState::StaConnected {
            // wait until we're no longer connected
            controller.wait_for_event(WifiEvent::StaDisconnected).await;
            Timer::after(Duration::from_millis(5000)).await
        }
        if !matches!(controller.is_started(), Ok(true)) {
            const SSID: &str = include_str!("../../secrets/ssid.txt");
            const PASSWORD: &str = include_str!("../../secrets/password.txt");

            let ssid = SSID.trim();
            let password = PASSWORD.trim();
            info!("ssid: {} <> password: {}", ssid, password);

            let ssid = SSID.try_into().unwrap();
            let password = PASSWORD.try_into().unwrap();
            let client_config = Configuration::Client(ClientConfiguration {
                ssid,
                password,
                ..Default::default()
            });
            controller.set_configuration(&client_config).unwrap();
            info!("Starting wifi");
            controller.start().await.unwrap();
            info!("Wifi started!");
        }
        info!("About to connect...");

        match controller.connect().await {
            Ok(_) => info!("Wifi connected!"),
            Err(e) => {
                info!("Failed to connect to wifi: {e:?}");
                Timer::after(Duration::from_millis(5000)).await
            }
        }
    }
}

#[embassy_executor::task]
async fn net_task(stack: &'static Stack<WifiDevice<'static, WifiStaDevice>>) {
    stack.run().await
}
