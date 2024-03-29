use crate::config::JsonTrait;

use crate::statics::*;
use crate::types::EthDevice;

use core::fmt::{self, Write};
use defmt::error;
use defmt::info;
use defmt::Debug2Format;

use embassy_net::{
    tcp::client::{TcpClient, TcpClientState},
    Stack,
};
use embassy_stm32::{peripherals::*, usart::Uart};
use embassy_time::{Duration, Instant, Timer};
use rust_mqtt::client::client_config::MqttVersion::*;

use miniserde::{json, Serialize};
use rust_mqtt::packet::v5::publish_packet::QualityOfService::{self, *};
use rust_mqtt::{
    client::{client::MqttClient, client_config::ClientConfig},
    utils::rng_generator::CountingRng,
};

const BUF_SIZE: usize = 1500;

#[derive(Clone, Debug)]
pub struct MqttMessage<'a> {
    pub topic: &'a str,
    pub payload: &'a str,
    pub qos: QualityOfService,
    pub retain: bool,
}

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

    use embedded_nal_async::{SocketAddr, TcpConnect};
    let state: TcpClientState<1, 2048, 2048> = TcpClientState::new();
    let client = TcpClient::new(stack, &state);

    let mqtt_config = crate::config::MqttConfig::builder()
        .host(env!("MQTTHOST"))
        .port(env!("MQTTPORT").parse().expect("Bad MQTT port in env"))
        .client_id(env!("MQTTCLIENTID"))
        .username(env!("MQTTUSERNAME"))
        .password(env!("MQTTPASSWORD"))
        .basetopic(env!("MQTTBASETOPIC"))
        .qos(env!("MQTTQOS").parse().expect("Bad QOS number in env"))
        .retain(env!("MQTTRETAIN") == "true")
        .interval(
            env!("MQTTINTERVAL")
                .parse()
                .expect("Bad MQTT interval number in env"),
        )
        .build();

    loop {
        info!("Setting up MQTT connection");

        let addr =
            SocketAddr::try_from(&mqtt_config).expect("MQTT host details are not valid IPv4");

        let retain = mqtt_config.get_retain();
        let qos = match mqtt_config.get_qos() {
            1 => QoS1,
            2 => QoS2,
            _ => QoS0,
        };
        let tcp = match client.connect(addr).await {
            Ok(t) => t,
            Err(e) => {
                error!("MQTT connect error: {}", e);
                Timer::after(Duration::from_secs(10)).await;
                continue;
            }
        };
        let mut config = ClientConfig::new(MQTTv5, CountingRng(50000));
        if mqtt_config.get_username().is_empty() {
            config.add_username(mqtt_config.get_username())
        };
        config.add_username(mqtt_config.get_username());
        if !mqtt_config.get_password().is_empty() {
            config.add_password(mqtt_config.get_password())
        };
        config.add_will(env!("MQTTWILLTOPIC"), b"Online", mqtt_config.get_retain());
        config.max_packet_size = 6000;
        config.keep_alive = 60000;
        config.max_packet_size = 300;
        let mut recv_buffer = [0; BUF_SIZE];
        let mut write_buffer = [0; BUF_SIZE];

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
                error!("{}", Debug2Format(&mqtt_config));
                Timer::after(Duration::from_secs(10)).await;
                continue;
            }
        };

        'inner: loop {
            info!("Setting up MQTT message");

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

            let p = match MQTTFMT.try_lock() {
                Ok(p) => p.device_update_msg(),
                Err(_) => {
                    defmt::error!("Cannot get mutex lock");
                    break 'inner;
                }
            };

            let message = MqttMessage {
                topic: mqtt_config.get_topic(),
                payload: p.as_str(),
                qos,
                retain,
            };

            // let mqtt_data = { *MQTTFMT.lock().await };
            if let Err(e) = client
                .send_message(
                    message.topic,
                    message.payload.as_bytes(),
                    message.qos,
                    message.retain,
                )
                .await
            {
                error!("MQTT send {}", e);
                break 'inner;
            }
            // rate limiter
            embassy_time::Timer::after(Duration::from_secs(mqtt_config.get_interval().into()))
                .await;
        }
        defmt::warn!("Dropping MQTT client");
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
                    if let Err(e) = config.decode_from_json(&buf[..len]) {
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

    fn device_update_msg(&self) -> alloc::string::String {
        json::to_string(&self)
    }
}
