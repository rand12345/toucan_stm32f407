use crc16::{State, MODBUS};
use defmt::*;
use embassy_futures::select::*;
use embassy_net::{tcp::TcpSocket, IpListenEndpoint};
use embassy_stm32::{
    gpio::{Level, Output, Speed},
    peripherals::PD7,
};
use embassy_time::{Duration, Timer};
use heapless::Vec as hVec;

use crate::{
    statics::LED_COMMAND,
    types::{StackType, RS485},
};

const TCP_TIMEOUT_SECS: u64 = 10;

#[embassy_executor::task]
pub async fn modbus_task(stack: StackType, serial: RS485<'static>, tx_en: PD7) {
    let mut serial = serial;
    let mut txen = Output::new(tx_en, Level::High, Speed::Medium);
    let num: &str = "502";
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
    info!("[{}] Spawning Modbus TCP socket", num);
    let mut rx_buffer = [0; 1024];
    let mut tx_buffer = [0; 1024];
    let mut socket = TcpSocket::new(stack, &mut rx_buffer, &mut tx_buffer);
    socket.set_timeout(Some(Duration::from_secs(TCP_TIMEOUT_SECS)));

    loop {
        LED_COMMAND.signal(crate::tasks::leds::LedCommand::Off(
            crate::tasks::leds::Led::Led3,
        ));
        match socket.state() {
            embassy_net::tcp::State::Closed => (),
            _ => {
                Timer::after(Duration::from_millis(100)).await;
                socket.close();
                info!("[{}] Modbus TCP socket closed", num);
                Timer::after(Duration::from_millis(100)).await;
                socket.abort();
            }
        };
        info!("[{}] Wait for connection...", num);
        let r = socket
            .accept(IpListenEndpoint {
                addr: None,
                port: 502,
            })
            .await;
        info!("[{}] Connected...", num);
        if let Err(e) = r {
            info!("[{}] connect error: {:?}", num, e);
            continue;
        }

        if let Some(ip) = socket.remote_endpoint() {
            info!("Accepted Modbus TCP client: {}:{}", ip.addr, ip.port);
        }
        const PAYLOADSIZE: usize = 512;
        let mut rtu_response: hVec<u8, PAYLOADSIZE> = hVec::new();
        let mut rtu_send_buf: [u8; 8] = [0; 8];
        let mut tcp_payload: hVec<u8, PAYLOADSIZE> = hVec::new();
        let mut tcp_recv_buf: [u8; 12] = [0; 12];
        #[allow(unused_assignments)]
        let mut crc: State<MODBUS> = State::new();
        if let Err(e) = serial.blocking_flush() {
            error!("RS485 flush {}", e)
        };
        'inner: loop {
            txen.set_high();
            LED_COMMAND.signal(crate::tasks::leds::LedCommand::Off(
                crate::tasks::leds::Led::Led3,
            ));

            if let Err(e) = socket.read(&mut tcp_recv_buf).await {
                error!("[{}] Modbus TCP {}", num, e);
                break 'inner;
            };
            LED_COMMAND.signal(crate::tasks::leds::LedCommand::On(
                crate::tasks::leds::Led::Led3,
            ));
            trace!("TCP >> STM {:x}", tcp_recv_buf);
            crc = State::<MODBUS>::new();
            crc.update(&tcp_recv_buf[6..12]);
            rtu_send_buf[0..6].copy_from_slice(&tcp_recv_buf[6..12]);
            rtu_send_buf[6..8].copy_from_slice(&crc.get().to_le_bytes());

            // send rtubuf
            trace!("STM >> MOD {:x}", rtu_send_buf);
            if let Err(e) = serial.write(&rtu_send_buf).await {
                error!("Serial send error 0 {}", e);
                break 'inner;
            };
            if let Err(_e) = serial.blocking_flush() {};
            txen.set_low();
            rtu_response.clear();

            // await serial read
            let mut buf = [0u8; 3];

            match select(
                serial.read_until_idle(&mut buf),
                Timer::after(Duration::from_millis(500)),
            )
            .await
            {
                Either::First(Ok(len)) => {
                    if len != 3 {
                        error!("Bad response from RTU");
                        continue;
                    }
                }
                Either::First(Err(e)) => {
                    error!("Serial read error 1 {}", e);
                    continue;
                }
                Either::Second(_) => {
                    error!("RS485 timeout");
                    continue;
                }
            };

            if buf[2] > 0x80 {
                warn!("Sending bad RTU as TCP");
                tcp_payload.clear();
                tcp_payload.extend_from_slice(&tcp_recv_buf[0..5]).unwrap();
                tcp_payload.push(0x3).unwrap();
                tcp_payload.extend_from_slice(&buf[0..3]).unwrap();
                warn!("Sending TCP {:x}", tcp_payload);
                if let Err(e) = socket.write(&tcp_payload).await {
                    error!("TCP send error 1 {}", e);
                    break 'inner;
                };
                continue;
            }
            trace!("Processing valid RTU response");
            if let Err(e) = rtu_response.extend_from_slice(&buf) {
                error!("Slice1 {}", e);
                continue;
            };
            let mut byte = [0];
            let count = buf[2] + 2;
            if count as usize > PAYLOADSIZE {
                error!("Payload overflow {} > {}", count, PAYLOADSIZE);
                continue;
            }

            for _ in 0..count {
                match select(
                    serial.read_until_idle(&mut byte),
                    Timer::after(Duration::from_millis(10)),
                )
                .await
                {
                    Either::First(Ok(_)) => rtu_response.push(byte[0]).unwrap(),
                    Either::First(Err(e)) => {
                        error!("Serial read error 2 {}", e);
                        continue;
                    }
                    Either::Second(_) => {
                        error!("RTU Read timeout");
                        continue;
                    }
                };
            }

            trace!("RTU response {:x}", rtu_response);
            let (response, crc_check) = rtu_response.split_at(rtu_response.len() - 2);
            crc = State::<MODBUS>::new();
            crc.update(response);
            let calc_crc = crc.get().to_le_bytes();
            if crc_check != calc_crc {
                error!(
                    "Serial read crc {:x} is invalid ({:x})",
                    crc_check, calc_crc
                );
                continue;
            }

            // src https://www.fernhillsoftware.com/help/drivers/modbus/modbus-protocol.html
            let rtu_data = &rtu_response[1..rtu_response.len()];
            if 7 + rtu_data.len() > PAYLOADSIZE {
                error!(
                    "TCP payload overflow {} > {}",
                    7 + rtu_data.len(),
                    PAYLOADSIZE,
                );
                continue;
            }
            tcp_payload.clear();
            tcp_payload.extend_from_slice(&tcp_recv_buf[0..5]).unwrap(); // tcp header
            tcp_payload.push(tcp_recv_buf[5] + 1).unwrap(); // address?
            tcp_payload.push(rtu_response[0]).unwrap(); // id

            tcp_payload.extend_from_slice(rtu_data).unwrap();

            trace!("TCP << Mod {:x}", tcp_payload);
            if let Err(e) = socket.write(tcp_payload.as_slice()).await {
                error!("TCP send error 2 {}", e);
                socket.close();
                break 'inner;
            }
            if let Err(e) = socket.flush().await {
                error!("TCP flush {}", e);
                socket.close();
                break 'inner;
            }
        }
    }
}
