use defmt::{error, info};

use embassy_futures::select::*;
use embassy_net::tcp::TcpSocket;
use embassy_stm32::{
    gpio::{Level, Output, Speed},
    peripherals::PD7,
};
use embassy_time::{Duration, Timer};

use crate::{
    tasks::modbus::{flow::*, models::*, ModbusError},
    types::{StackType, RS485},
};

const TCP_TIMEOUT_SECS: u64 = 30;

#[embassy_executor::task]
pub async fn modbus_task(stack: StackType, serial: RS485<'static>, tx_en: PD7) {
    let tx_enable_pin = Output::new(tx_en, Level::Low, Speed::Medium);
    let num: &str = "502";
    loop {
        if stack.is_link_up() {
            if let Some(_config) = stack.config_v4() {
                break;
            }
        }
        Timer::after(Duration::from_millis(500)).await;
    }
    #[cfg(feature = "modbus_client")]
    let sock: &str = env!("MODBUS_REMOTE");
    #[cfg(feature = "modbus_bridge")]
    let sock: &str = "0.0.0.0:502";

    let tcpmode: ModbusTcpMode = {
        let sock: no_std_net::SocketAddr = sock
            .parse()
            .expect("Modbus remote address:port error - see MODBUS_REMOTE");
        ModbusTcpMode::Client(sock)
    };

    let mut modbus_rtu: ModbusRtu<'_> = ModbusRtu::new(serial, tx_enable_pin, tcpmode);
    // Await client loop
    info!("[:{}] Spawning Modbus TCP socket", num);
    loop {
        defmt::warn!("Loop");
        let mut rx_buffer = [0; 1024];
        let mut tx_buffer = [0; 1024];
        let mut socket = TcpSocket::new(stack, &mut rx_buffer, &mut tx_buffer);
        socket.set_timeout(Some(Duration::from_secs(TCP_TIMEOUT_SECS)));

        let mut modbus_tcp: ModbusTcp<'_> = ModbusTcp::new(socket, tcpmode, 1000);

        #[cfg(feature = "modbus_client")]
        {
            if let Err(e) = modbus_tcp.connect().await {
                error!("Modbus TCP accept error {}", e);
                modbus_tcp.reset().await;
                embassy_time::Timer::after(Duration::from_secs(2)).await; //anti-hammer
                continue;
            };
            if let Some(client) = modbus_tcp.connected_client().await {
                info!(
                    "Accepted Modbus TCP server: {}:{}",
                    client.addr, client.port
                );
            };
        }

        #[cfg(feature = "modbus_bridge")]
        {
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
        }

        'inner: loop {
            #[cfg(any(feature = "modbus_bridge", feature = "modbus_client"))]
            let process = process_flow(&mut modbus_tcp, &mut modbus_rtu);
            let timeout = embassy_time::Timer::after(Duration::from_secs(TCP_TIMEOUT_SECS));
            // Client connected loop
            match select(process, timeout).await {
                Either::First(Ok(_)) => defmt::info!("Process OK"), // Client request processed Ok
                Either::First(Err(ModbusError::TcpRxFail(c))) => {
                    error!("TCP client left - breaking tcp loop {}", c);
                    break 'inner;
                }
                Either::First(Err(ModbusError::ReadExactError)) => {
                    break 'inner;
                }
                Either::First(Err(e)) => {
                    error!("Modbus error: {}", e);
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
