use defmt::{error, info};

use embassy_futures::select::*;
use embassy_net::tcp::TcpSocket;
use embassy_stm32::{
    gpio::{Level, Output, Speed},
    peripherals::PD7,
};
use embassy_time::{Duration, Timer};

use crate::{
    tasks::modbus::{modbus_data::process, ModbusError},
    types::{StackType, RS485},
};

use super::modbus_data::{ModbusRtu, ModbusTcp};

pub const RX_TIMEOUT_BYTE: u64 = 200;

const TCP_TIMEOUT_SECS: u64 = 3;

#[embassy_executor::task]
pub async fn modbus_task(stack: StackType, serial: RS485<'static>, tx_en: PD7) {
    let serial = serial;
    let txen = Output::new(tx_en, Level::High, Speed::Medium);
    let num: &str = "502";
    loop {
        if stack.is_link_up() {
            break;
        }
        Timer::after(Duration::from_millis(500)).await;
    }
    loop {
        if let Some(_config) = stack.config_v4() {
            break;
        }
        Timer::after(Duration::from_millis(500)).await;
    }
    let mut modbus_rtu = ModbusRtu::new(serial, txen);
    loop {
        // Await client loop
        info!("[:{}] Spawning Modbus TCP socket", num);
        let mut rx_buffer = [0; 1024];
        let mut tx_buffer = [0; 1024];
        let mut socket = TcpSocket::new(stack, &mut rx_buffer, &mut tx_buffer);
        socket.set_timeout(Some(Duration::from_secs(TCP_TIMEOUT_SECS)));
        let mut modbus_tcp = ModbusTcp::new(socket, 502u16, 1000);
        if let Err(e) = modbus_tcp.wait_connection().await {
            error!("Modbus TCP accept error {}", e);
            modbus_tcp.reset().await;
            continue;
        };
        if let Some(client) = modbus_tcp.connected_client().await {
            info!(
                "Accepted Modbus TCP client: {}:{}",
                client.addr, client.port
            );
        };

        'inner: loop {
            // Client connected loop
            let timeout = embassy_time::Timer::after(Duration::from_secs(TCP_TIMEOUT_SECS));
            match select(process(&mut modbus_tcp, &mut modbus_rtu), timeout).await {
                Either::First(Ok(_)) => (), // Client request processed Ok
                Either::First(Err(ModbusError::TcpRxFail(_))) => {
                    error!("TCP client left - breaking tcp loop");
                    break 'inner;
                }
                Either::First(Err(ModbusError::Tcp)) => {
                    error!("Modbus TCP timeout");
                    // drop client
                    break 'inner;
                }
                Either::First(Err(e)) => {
                    error!("Modbus error {}", e);
                }
                Either::Second(_) => {
                    error!("Modbus TCP timeout timer hit");
                    break 'inner;
                }
            }
        }
        modbus_tcp.reset().await;
        drop(modbus_tcp);
    }
}
