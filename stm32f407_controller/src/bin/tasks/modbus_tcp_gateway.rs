use core::fmt::Error;

use crc16::{State, MODBUS};
use defmt::*;
use embassy_futures::select::*;
use embassy_net::{
    tcp::{self, AcceptError, TcpSocket},
    IpListenEndpoint,
};
use embassy_stm32::{
    gpio::{AnyPin, Level, Output, Speed},
    peripherals::PD7,
    usart,
};
use embassy_time::{Duration, Timer};
use heapless::Vec as hVec;

use crate::{
    statics::LED_COMMAND,
    types::{StackType, RS485},
};

const TCP_TIMEOUT_SECS: u64 = 10;

struct ModbusTcp<'a> {
    // TCP-specific fields
    socket: TcpSocket<'a>,
    tcp_port: u16,
    timeout: u64,
    tcp_recv_buf: TcpRxPayload,
    tcp_transmit_buf: TcpTxPayload,
}

impl<'a> ModbusTcp<'a> {
    // TCP-specific methods
    fn new(socket: TcpSocket<'a>, tcp_port: impl Into<u16>, timeout_ms: u64) -> Self {
        ModbusTcp {
            socket,
            tcp_port: tcp_port.into(),
            timeout: timeout_ms,
            tcp_recv_buf: TcpRxPayload([0; 12]),
            tcp_transmit_buf: TcpTxPayload(hVec::new()),
        }
    }

    pub async fn reset(&mut self) -> &mut Self {
        match self.socket.state() {
            embassy_net::tcp::State::Closed => (),
            _ => {
                Timer::after(Duration::from_millis(self.timeout)).await;
                self.socket.close();
                info!("Modbus TCP socket closed");
                Timer::after(Duration::from_millis(100)).await;
                self.socket.abort();
            }
        };
        self
    }

    pub async fn wait_connection(&mut self) -> Result<&mut Self, AcceptError> {
        self.socket
            .accept(IpListenEndpoint {
                addr: None,
                port: self.tcp_port,
            })
            .await?;
        Ok(self)
    }

    async fn read(&mut self) -> Result<usize, tcp::Error> {
        self.socket.read(&mut self.tcp_recv_buf.0).await
    }
    async fn write(&mut self, payload: TcpTxPayload) -> Result<usize, tcp::Error> {
        self.socket.write(&payload.0).await
    }
    pub async fn connected_client(&mut self) -> Option<embassy_net::IpEndpoint> {
        self.socket.remote_endpoint()
    }
}

struct TcpRxPayload([u8; 12]);
struct TcpTxPayload(hVec<u8, 512>);
struct RtuRxPayload(hVec<u8, 512>);
struct RtuTxPayload([u8; 8]);

struct ModbusRtu<'a> {
    // RTU-specific fields
    serial: RS485<'static>,
    tx_en: Output<'a, PD7>,
    // other fields...
}

impl<'a> ModbusRtu<'a> {
    // RTU-specific methods
    fn new(serial: RS485<'static>, tx_en: Output<'static, PD7>) -> Self {
        ModbusRtu { serial, tx_en }
    }

    pub async fn send_and_receive(
        &mut self,
        tcp_request: &TcpRxPayload,
    ) -> Result<TcpTxPayload, usart::Error> {
        self.write(tcp_request.into()).await;
        let r = match self.read(tcp_request).await? {
            (Some(rtu_rx), tcp_tx) => TcpTxPayload::from(rtu_rx),
            (None, tx) => tx,
        };
        Ok(r)
    }
    pub async fn write(&mut self, payload: RtuTxPayload) -> Result<(), usart::Error> {
        self.tx_en.set_high();
        self.serial.blocking_flush()?;
        self.serial.write(&payload.0).await?;
        self.tx_en.set_low();
        Ok(())
    }

    async fn read(
        &mut self,
        tcp_request: &TcpRxPayload,
    ) -> Result<(Option<RtuRxPayload>, TcpTxPayload), usart::Error> {
        self.tx_en.set_low();
        let mut tcp_tx: TcpTxPayload = TcpTxPayload(hVec::new());
        let mut rx_buf: RtuRxPayload = RtuRxPayload(hVec::new()); //= ;
        self.serial.read_until_idle(&mut rx_buf.0[0..3]).await?;
        if rx_buf.0[2] > 0x80 {
            tcp_tx.0.extend_from_slice(&tcp_request.0[0..5]).unwrap();
            tcp_tx.0.push(0x3).unwrap();
            tcp_tx.0.extend_from_slice(&rx_buf.0[0..3]).unwrap();
            error!("Bad modbus address");
            return Ok((None, tcp_tx));
        };
        // tcp_tx.0.extend_from_slice(&rx_buf.0);
        let count = (rx_buf.0[2] + 2) as usize;
        let mut byte = [0];
        for _ in 0..count {
            match select(
                self.serial.read_until_idle(&mut byte),
                Timer::after(Duration::from_millis(10)),
            )
            .await
            {
                Either::First(Ok(_)) => rx_buf.0.push(byte[0]).unwrap(),
                Either::First(Err(e)) => {
                    error!("Serial read error 2 {}", e);
                    return Err(usart::Error::Framing);
                }
                Either::Second(_) => {
                    error!("RTU Read timeout");
                    return Err(usart::Error::Overrun);
                }
            };
        }
        let (response, crc_check) = rx_buf.0.split_at(rx_buf.0.len() - 2);
        let mut crc = State::<MODBUS>::new();
        crc.update(response);
        let calc_crc = crc.get().to_le_bytes();
        if crc_check != calc_crc {
            error!(
                "Serial read crc {:x} is invalid ({:x})",
                crc_check, calc_crc
            );
        }
        Ok((Some(rx_buf), tcp_tx))
    }

    // other methods...
}

impl TryFrom<(RtuRxPayload, TcpRxPayload)> for TcpTxPayload {
    type Error = Error;
    fn try_from(payloads: (RtuRxPayload, TcpRxPayload)) -> Result<Self, Error> {
        let (rtu_response, tcp_recv_buf) = payloads;
        let mut output: hVec<u8, 512> = hVec::new();
        output.extend_from_slice(&tcp_recv_buf.0[0..5]).unwrap(); // tcp header
        output.push(tcp_recv_buf.0[5] + 1).unwrap(); // address
        output.push(rtu_response.0[0]).unwrap(); // id
        output
            .extend_from_slice(&rtu_response.0[1..rtu_response.0.len()])
            .unwrap();
        Ok(TcpTxPayload(output))
    }
}

impl From<&TcpRxPayload> for RtuTxPayload {
    fn from(tcp: &TcpRxPayload) -> Self {
        let mut output: [u8; 8] = [0; 8];
        let mut crc: State<MODBUS> = State::new();
        crc.update(&tcp.0[6..12]);
        output[0..6].copy_from_slice(&tcp.0[6..12]);
        output[6..8].copy_from_slice(&crc.get().to_le_bytes());
        RtuTxPayload(output)
    }
}

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
    let mut modbus_tcp = ModbusTcp::new(socket, 502u16, 1000);
    let mut modbus_rtu = ModbusRtu::new(serial, txen);
    loop {
        modbus_tcp.read().await.unwrap();
        let rtu_tx_payload = RtuTxPayload::from(&modbus_tcp.tcp_recv_buf);
        modbus_rtu.write(rtu_tx_payload).await;
        let mut modbus_rtu_rx = RtuRxPayload(hVec::new());
        modbus_rtu
            .read(modbus_rtu_rx, &modbus_tcp.tcp_recv_buf)
            .await;
    }
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
        'inner: {
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
                        break 'inner;
                    }
                }
                Either::First(Err(e)) => {
                    error!("Serial read error 1 {}", e);
                    break 'inner;
                }
                Either::Second(_) => {
                    error!("RS485 timeout");
                    break 'inner;
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
                break 'inner;
            }
            trace!("Processing valid RTU response");
            if let Err(e) = rtu_response.extend_from_slice(&buf) {
                error!("Slice1 {}", e);
                break 'inner;
            };
            let mut byte = [0];
            let count = buf[2] + 2;
            if count as usize > PAYLOADSIZE {
                error!("Payload overflow {} > {}", count, PAYLOADSIZE);
                break 'inner;
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
                        break;
                    }
                    Either::Second(_) => {
                        error!("RTU Read timeout");
                        break;
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
                break 'inner;
            }

            // src https://www.fernhillsoftware.com/help/drivers/modbus/modbus-protocol.html
            let rtu_data = &rtu_response[1..rtu_response.len()];
            if 7 + rtu_data.len() > PAYLOADSIZE {
                error!(
                    "TCP payload overflow {} > {}",
                    7 + rtu_data.len(),
                    PAYLOADSIZE,
                );
                break 'inner;
            }
            tcp_payload.clear();
            tcp_payload.extend_from_slice(&tcp_recv_buf[0..5]).unwrap(); // tcp header
            tcp_payload.push(tcp_recv_buf[5] + 1).unwrap(); // address
            tcp_payload.push(rtu_response[0]).unwrap(); // id

            tcp_payload.extend_from_slice(rtu_data).unwrap();

            trace!("TCP << Mod {:x}", tcp_payload);
            if let Err(e) = socket.write(tcp_payload.as_slice()).await {
                error!("TCP send error 2 {}", e);
                break 'inner;
            }
            if let Err(e) = socket.flush().await {
                error!("TCP flush {}", e);
                break 'inner;
            }
        }
    }
}
