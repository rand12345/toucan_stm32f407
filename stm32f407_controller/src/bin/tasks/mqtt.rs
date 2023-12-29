use crate::config::JsonTrait;
use crate::errors::StmError;
use crate::statics::*;
use crate::types::EthDevice;
use alloc::string::ToString;
use core::fmt::{self, Write};
use core::str::FromStr;
use defmt::error;
use defmt::info;
use defmt::Debug2Format;
use dotenvy_macro::dotenv;
use embassy_net::tcp::client::{TcpClient, TcpClientState};
use embassy_net::Stack;
use embassy_stm32::peripherals::*;
use embassy_stm32::usart::Uart;
use embassy_time::{Duration, Instant, Timer};
use embedded_nal_async::IpAddr;
use embedded_nal_async::Ipv4Addr;
use embedded_nal_async::SocketAddr;
use miniserde::__private::String;
use miniserde::{json, Serialize};
use rust_mqtt::packet::v5::publish_packet::QualityOfService::*;
use rust_mqtt::{
    client::{client::MqttClient, client_config::ClientConfig},
    utils::rng_generator::CountingRng,
};

const BUF_SIZE: usize = 1500;

#[embassy_executor::task]
pub async fn mqtt_net_task(stack: &'static Stack<EthDevice>) {
    // let mut messagebus = MESSAGEBUS.subscriber().unwrap();
    loop {
        if stack.is_link_up() {
            break;
        }
        Timer::after(Duration::from_millis(500)).await;
    }

    info!("Waiting to get IP address...");
    loop {
        if let Some(_config) = stack.config_v4() {
            break;
        }
        Timer::after(Duration::from_millis(1500)).await;
    }
    info!("Spawning MQTT client");
    static STATE: TcpClientState<1, 2048, 2048> = TcpClientState::new();
    let client = TcpClient::new(stack, &STATE);

    let nvs_config = crate::config::MqttConfig::new(
        Some(dotenv!("host").to_string()),
        dotenv!("port").parse().ok(),
        Some(dotenv!("client_id").to_string()),
        Some(dotenv!("username").to_string()),
        Some(dotenv!("password").to_string()),
        Some(dotenv!("topic").to_string()),
        1,
        dotenv!("retain") == "true",
        dotenv!("interval").parse().unwrap_or(10),
    );
    let ip = Ipv4Addr::from_str(nvs_config.host.as_ref().unwrap()).map_err(|_| StmError::BadMqttIp);

    if ip.is_err() {
        panic!("Bad mqtt IP");
    }
    if nvs_config.port.is_none() {
        panic!("Bad mqtt port");
    }
    let addr = SocketAddr::new(IpAddr::V4(ip.unwrap()), nvs_config.port.unwrap());

    loop {
        info!("Setting up MQTT connection");
        use embedded_nal_async::TcpConnect;

        let tcp = match client.connect(addr).await {
            Ok(t) => t,
            Err(e) => {
                error!("MQTT connect error: {}", e);
                Timer::after(Duration::from_secs(10)).await;
                continue;
            }
        };

        use rust_mqtt::client::client_config::MqttVersion::*;

        let mut config = ClientConfig::new(MQTTv5, CountingRng(50000));
        config.add_username(nvs_config.username.as_ref().unwrap());
        config.add_password(nvs_config.password.as_ref().unwrap());
        config.max_packet_size = 6000;
        config.keep_alive = 60000;
        config.max_packet_size = 300;
        let mut recv_buffer = [0; BUF_SIZE];
        let mut write_buffer = [0; BUF_SIZE];

        // use embedded_io::asynch::Read;
        let mut client = MqttClient::<_, 5, _>::new(
            tcp,
            &mut write_buffer,
            BUF_SIZE,
            &mut recv_buffer,
            BUF_SIZE,
            config,
        );
        match client.connect_to_broker().await {
            Ok(_) => info!("MQTT connected ok"),
            Err(e) => {
                error!("MQTT Failed {}", e);
                error!("{}", Debug2Format(&nvs_config));
                // break;
                Timer::after(Duration::from_secs(10)).await;
                continue;
            }
        };

        'inner: loop {
            info!("Setting up MQTT message");
            // let message = match messagebus.next_message_pure().await {
            //     crate::types::messagebus::Message::Mqtt(message) => message,
            //     _ => continue,
            // };

            // let _ = SEND_MQTT.wait().await;
            //
            if let Err(e) = client.send_ping().await {
                error!("No response from MQTT server {}", e);
                match client.disconnect().await {
                    Ok(_) => info!("MQTT disconnected ok"),
                    Err(e) => error!("MQTT disconnect failed {}", e),
                };
                match client.connect_to_broker().await {
                    Ok(_) => info!("MQTT reconnected ok"),
                    Err(e) => error!("MQTT reconnect failed {}", e),
                };
                break 'inner;
            }

            let mut topic: heapless::String<50> = heapless::String::new();
            let _ = topic.push_str("test_data"); // temp debug
            let mut payload: heapless::String<512> = heapless::String::new();
            let p = match MQTTFMT.try_lock() {
                Ok(p) => p.device_update_msg(),
                Err(_) => {
                    defmt::error!("Cannot get mutex lock");
                    break 'inner;
                }
            };
            if p.len() > 512 {
                error!("JSON payload {} > 512", p.len());
                continue;
            }
            payload.push_str(&p).unwrap();
            let qos = match nvs_config.qos {
                1 => QoS1,
                2 => QoS2,
                _ => QoS0,
            };
            let message = crate::types::messagebus::MqttMessage {
                topic,
                payload,
                qos,
                retain: nvs_config.retain,
            };

            // let mqtt_data = { *MQTTFMT.lock().await };
            if let Err(e) = client
                .send_message(
                    &message.topic,
                    message.payload.as_bytes(),
                    message.qos,
                    message.retain,
                )
                // .send_message(topic, mqtt_data.device_update_msg().as_bytes(), qos, retain)
                .await
            {
                error!("MQTT send {}", e);
                break 'inner;
            }
            // rate limiter
            embassy_time::Timer::after(Duration::from_secs(nvs_config.interval.into())).await;
        }
        defmt::warn!("Dropping MQTT client");
        // drop(client);
    }
}

#[embassy_executor::task]
// pub async fn uart_task(uart: Uart<'static, USART3, DMA1_CH2, DMA1_CH3>) {
pub async fn uart_task(uart: Uart<'static, USART6, DMA2_CH7, DMA2_CH2>) {
    use embassy_futures::select::{select, Either};
    let mut uart = uart;
    if uart.blocking_flush().is_err() {
        panic!();
    };
    let (mut tx, mut rx) = uart.split();
    let mut buf = [0_u8; 512];
    let mut mqtt_frequency = Instant::now();

    loop {
        match select(rx.read_until_idle(&mut buf), SEND_MQTT.wait()).await {
            Either::First(read) => match read {
                Ok(len) => {
                    let mut config = CONFIG.lock().await;
                    if let Err(e) = config.from_json(&buf[..len]) {
                        let message = if let Ok(message) = core::str::from_utf8(&buf[..len]) {
                            message
                        } else {
                            "(unable to decode utf-8)"
                        };
                        error!(
                            "UART deserialise bytes error {}: {}",
                            Debug2Format(&e),
                            message
                        );
                        let _ = tx.write(r#"{error: ""#.as_bytes()).await;
                        let _ = tx.write(&buf[..len]).await;
                        let _ = tx.write(r#""}"#.as_bytes()).await;
                        Timer::after(Duration::from_millis(500)).await;
                        let _ = tx.write(config.to_json().as_bytes()).await;
                    } else {
                        info!("Config updated from UART");
                        let _ = tx.write(config.to_json().as_bytes()).await;
                        // update bms
                        let mut bms = BMS.lock().await;
                        bms.config = config.export_as_bms();
                    };
                    buf = [0_u8; 512];
                }
                Err(_) => continue,
            },
            Either::Second(_) => {
                if mqtt_frequency.elapsed().as_secs() < LAST_READING_TIMEOUT_SECS {
                    continue;
                }
                mqtt_frequency = Instant::now();
                buf = [0_u8; 512];
                let mqtt_data = MQTTFMT.lock().await;
                if let Err(e) = tx.write(mqtt_data.device_update_msg().as_bytes()).await {
                    error!("UART send bytes error {}", Debug2Format(&e));
                } else {
                    info!("MQTT sent to UART")
                };
            }
        }
    }
}

struct SliceWriter<'a>(&'a mut [u8]);

impl<'a> Write for SliceWriter<'a> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        let bytes = s.as_bytes();
        if self.0.len() < bytes.len() {
            Err(fmt::Error)
        } else {
            let (head, tail) = core::mem::take(&mut self.0).split_at_mut(bytes.len());
            head.copy_from_slice(bytes);
            self.0 = tail;
            Ok(())
        }
    }
}

#[derive(Clone, Copy, Serialize)]
pub struct MqttFormat {
    soc: f32,
    volts: f32,
    cell_mv_high: u16,
    cell_mv_low: u16,
    cell_temp_high: f32,
    cell_temp_low: f32,
    // #[serde(with = "BigArray")]
    // #[serde(skip)]
    // cells_millivolts: [u16; 96],
    // #[serde(skip)]
    // #[serde(with = "BigArray")]
    // cell_balance: [bool; 96],
    amps: f32,
    kwh: f32,
    charge: f32,
    discharge: f32,
    bal: u8,
    valid: bool,
}

impl From<bms_standard::Bms> for MqttFormat {
    fn from(bmsdata: bms_standard::Bms) -> Self {
        MqttFormat {
            soc: bmsdata.soc,
            volts: bmsdata.pack_volts,
            cell_mv_high: *bmsdata.cell_range_mv.maximum(),
            cell_mv_low: *bmsdata.cell_range_mv.minimum(),
            cell_temp_high: *bmsdata.temps.maximum(),
            cell_temp_low: *bmsdata.temps.minimum(),
            // cells_millivolts : bmsdata.cells;
            // cell_balance  bmsdata.bal_cells;
            amps: bmsdata.current,
            kwh: bmsdata.kwh_remaining,
            charge: bmsdata.charge_max,
            discharge: bmsdata.discharge_max,
            bal: bmsdata.get_balancing_cells(),
            valid: bmsdata.valid,
        }
    }
}

impl MqttFormat {
    pub fn default() -> Self {
        Self {
            soc: 0.0,
            volts: 0.0,
            cell_mv_high: 0,
            cell_mv_low: 0,
            cell_temp_high: 0.0,
            cell_temp_low: 0.0,
            // cells_millivolts: [0; 96],
            // cell_balance: [false; 96],
            amps: 0.0,
            kwh: 0.0,
            charge: 0.0,
            discharge: 0.0,
            bal: 0,
            valid: false,
        }
    }

    fn device_update_msg(&self) -> String {
        json::to_string(&self)
    }
}

// let addr = match addr {
//     Ok(addr) => addr,
//     Err(e) => {
//         error!("MQTT init killed, bad config {}", e);
//         return;
//     }
// };
// let nvs_config = MQTTCONFIG.lock().await;

// let retain = nvs_config.retain;
// let topic = if nvs_config.basetopic.is_none() {
//     ""
// } else {
//     nvs_config.basetopic.as_ref().unwrap()
// };
// let client_id = if nvs_config.client_id.is_none() {
//     "ToucanBmsGateway"
// } else {
//     nvs_config.client_id.as_ref().unwrap()
// };

// let mut config = ClientConfig::new(
//     rust_mqtt::client::client_config::MqttVersion::MQTTv5,
//     CountingRng(20000),
// );

// config.add_client_id(client_id);

// if nvs_config.username.is_some() {
//     config.add_username(nvs_config.username.as_ref().unwrap())
// };
// if nvs_config.password.is_some() {
//     config.add_password(nvs_config.username.as_ref().unwrap())
// };

// let qos = match nvs_config.qos {
//     1 => QoS1,
//     2 => QoS2,
//     _ => QoS0,
// };

// let interval = nvs_config.interval;
// config.keep_alive = u16::MAX;
// info!("connecting...");

// let addr = async || -> Result<SocketAddr, StmError> {
//     let mut nvs_config = MQTTCONFIG.lock().await;

//     if nvs_config.host.is_none() {
//         return Err(StmError::BadMqttIp);
//     }
//     if nvs_config.port.is_none() {
//         nvs_config.port = Some(502)
//     }
//     if nvs_config.port.unwrap() > 65354 {
//         return Err(StmError::BadMqttPort);
//     }
//     let ip = Ipv4Addr::from_str(nvs_config.host.as_ref().unwrap())
//         .map_err(|_| StmError::BadMqttIp)?;

//     Ok(SocketAddr::new(IpAddr::V4(ip), nvs_config.port.unwrap()))
// };
