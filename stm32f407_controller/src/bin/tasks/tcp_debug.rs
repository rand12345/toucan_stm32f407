use defmt::*;
use embassy_futures::select::*;
use embassy_net::{tcp::TcpSocket, IpListenEndpoint};
use embassy_time::{Duration, Instant, Timer};

use crate::{
    statics::{BMS_CHANNEL_RX, BMS_CHANNEL_TX, LED_COMMAND},
    types::StackType,
    utils::ByteMutWriter,
};

const TCP_TIMEOUT_SECS: u64 = 10;
const TCP_PORT: u16 = 23;

#[embassy_executor::task]
pub async fn debug_task(stack: StackType) {
    loop {
        if stack.is_link_up() {
            break;
        }
        Timer::after(Duration::from_millis(500)).await;
    }

    // info!("Waiting to get IP address...");
    loop {
        if let Some(_config) = stack.config_v4() {
            break;
        }
        Timer::after(Duration::from_millis(500)).await;
    }
    info!("[{}] Spawning Modbus TCP socket", TCP_PORT);
    let mut rx_buffer = [0; 1024];
    let mut tx_buffer = [0; 1024];
    let mut socket = TcpSocket::new(stack, &mut rx_buffer, &mut tx_buffer);
    socket.set_timeout(Some(Duration::from_secs(TCP_TIMEOUT_SECS)));

    let bms_rx_listener = BMS_CHANNEL_RX.receiver();
    let bms_tx_listener = BMS_CHANNEL_TX.receiver();

    loop {
        LED_COMMAND.signal(crate::tasks::leds::LedCommand::Off(
            crate::tasks::leds::Led::Led3,
        ));
        match socket.state() {
            embassy_net::tcp::State::Closed => (),
            _ => {
                Timer::after(Duration::from_millis(100)).await;
                socket.close();
                info!("[{}] Debug TCP socket closed", TCP_PORT);
                Timer::after(Duration::from_millis(100)).await;
                socket.abort();
            }
        };
        info!("[{}] Wait for connection...", TCP_PORT);
        let r = socket
            .accept(IpListenEndpoint {
                addr: None,
                port: TCP_PORT,
            })
            .await;
        info!("[{}] Debug Connected...", TCP_PORT);
        if let Err(e) = r {
            info!("[{}] connect error: {:?}", TCP_PORT, e);
            continue;
        }

        if let Some(ip) = socket.remote_endpoint() {
            info!("Accepted Debug TCP client: {}:{}", ip.addr, ip.port);
        }
        let mut buf = [0u8; 256];
        let mut buf = ByteMutWriter::new(&mut buf);
        let time = Instant::now();
        'inner: loop {
            let (label, frame) =
                match select(bms_rx_listener.receive(), bms_tx_listener.receive()).await {
                    Either::First(f) => ("BMS Rx", f),
                    Either::Second(f) => ("BMS Tx", f),
                };

            let (id, data) = (frame.id(), frame.data());

            if core::fmt::write(
                &mut buf,
                format_args!(
                    "{} {}ms {:x?} {:x?}\n",
                    label,
                    time.elapsed().as_millis(),
                    id,
                    data
                ),
            )
            .is_ok()
            {
                if let Err(e) = socket.write(buf.buf).await {
                    error!("Debug TCP send {}", e);
                    socket.close();
                    break 'inner;
                };
            };
            //BMS Tx 53328ms Standard(StandardId(423)) Some(Data { len: 8, bytes: [33, 7f, ff, ff, ff, e0, ff, ff] })
            buf.clear()
        }
    }
}
