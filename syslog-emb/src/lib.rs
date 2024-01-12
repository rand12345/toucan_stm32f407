#![no_std]
#![allow(dead_code)]
#![feature(error_in_core)]

//!  // temp UdpSocket
//!  let mut rx_buffer = [0; 512];
//!  let mut tx_buffer = [0; 512];
//!  let mut rx_meta = [embassy_net::udp::PacketMetadata::EMPTY; 16];
//!  let mut tx_meta = [embassy_net::udp::PacketMetadata::EMPTY; 16];
//!  let mut socket = embassy_net::udp::UdpSocket::new(
//!      stack,
//!      &mut rx_meta,
//!      &mut rx_buffer,
//!      &mut tx_meta,
//!      &mut tx_buffer,
//!  );
//!  let ip = embassy_net::IpEndpoint::new(
//!      embassy_net::IpAddress::Ipv4("10.0.1.72".parse().unwrap()),
//!      514,
//!  );
//!  if let Err(e) = socket.bind(ip.port) {
//!      error!("bind error: {}", e);
//!  };
//!  let syslog_socket = SyslogSocket::Udp(socket, ip);
//!
//!  loop {
//!     let datetime: embassy_stm32::rtc::DateTime = statics::UTC_NOW.wait().await;
//!     let message = SyslogMessage {
//!         priority: 1,
//!         hostname: "FooHost",
//!         source: "FooSource",
//!         proc_id: "123",
//!         message: "My test",
//!         message_data: "Some=123",
//!         datetime,
//!     };
//!     syslog.send(&mut buf, message).await;
//!     buf.clear();
//!     embassy_time::Timer::after(embassy_time::Duration::from_secs(10)).await;
//!   }
//!
//!
//!
//!
use core::{str::FromStr, sync::atomic::AtomicU32};

use embassy_net::{
    tcp::{Error as TcpError, TcpSocket},
    udp::{SendError as UdpError, UdpSocket},
    IpEndpoint,
};

use defmt::Format;
use embassy_stm32::rtc::DateTime;

#[derive(Debug, Format)]
pub enum SyslogError {
    ConnectionReset,
    NoRoute,
    SocketNotBound,
}
impl core::error::Error for SyslogError {}
impl core::fmt::Display for SyslogError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            SyslogError::ConnectionReset => write!(f, "ConnectionReset"),
            SyslogError::NoRoute => write!(f, "NoRoute"),
            SyslogError::SocketNotBound => write!(f, "SocketNotBound"),
        }
    }
}

impl From<TcpError> for SyslogError {
    fn from(err: TcpError) -> Self {
        match err {
            TcpError::ConnectionReset => SyslogError::ConnectionReset,
        }
    }
}
impl From<UdpError> for SyslogError {
    fn from(err: UdpError) -> Self {
        match err {
            UdpError::NoRoute => SyslogError::NoRoute,
            UdpError::SocketNotBound => SyslogError::SocketNotBound,
        }
    }
}

pub enum SyslogSocket<'a> {
    Tcp(TcpSocket<'a>),
    Udp(UdpSocket<'a>, IpEndpoint),
}

pub struct SyslogMessage<'a> {
    pub priority: u8,
    pub hostname: &'a str,
    pub source: &'a str,
    pub proc_id: &'a str,
    pub message: &'a str,
    pub message_data: &'a str,
    pub datetime: DateTime,
}

pub struct Syslog<'a> {
    socket: SyslogSocket<'a>,
}

static mut MSG_COUNTER: AtomicU32 = AtomicU32::new(0);

impl<'a> Syslog<'a> {
    pub fn new(socket: SyslogSocket<'a>) -> Self {
        Self { socket }
    }
    pub async fn send<T: core::fmt::Write + core::borrow::Borrow<[u8]>>(
        &mut self,
        buf: &mut T,
        message: SyslogMessage<'a>,
    ) -> Result<(), SyslogError> {
        let counter = unsafe { MSG_COUNTER.fetch_add(1, core::sync::atomic::Ordering::Relaxed) };
        let mut time: ByteMutWriterCap<50> = ByteMutWriterCap::new();
        use core::fmt::Write;
        let dt = message.datetime;
        write!(
            time,
            "{}-{}-{}T{}:{}:{}.0Z",
            dt.year(),
            dt.month(),
            dt.day(),
            dt.hour(),
            dt.minute(),
            dt.second()
        )
        .unwrap();
        writeln!(
            buf,
            "<{}>1 {} {} {} ID{} [{}] {}",
            message.priority,
            time.as_str(),
            message.hostname,
            message.proc_id,
            counter,
            message.message_data,
            message.message
        )
        .unwrap();
        self.transmit(buf.borrow()).await
    }
    async fn transmit(&mut self, data: &[u8]) -> Result<(), SyslogError> {
        match &mut self.socket {
            SyslogSocket::Tcp(socket) => {
                socket.write(data).await?;
            }
            SyslogSocket::Udp(socket, ip) => {
                socket.send_to(data, *ip).await?;
            }
        };
        Ok(())
    }
}

pub fn add(left: usize, right: usize) -> usize {
    left + right
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        let result = add(2, 2);
        assert_eq!(result, 4);
    }
}

pub struct ByteMutWriterCap<const N: usize> {
    pub buf: [u8; N],
    pub cursor: usize,
}

#[allow(dead_code, clippy::new_without_default)]
impl<const N: usize> ByteMutWriterCap<N> {
    pub fn new() -> Self {
        ByteMutWriterCap {
            buf: [0; N],
            cursor: 0,
        }
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn as_str(&self) -> &str {
        use core::str;
        str::from_utf8(&self.buf[0..self.cursor]).unwrap()
    }

    pub fn capacity(&self) -> usize {
        self.buf.len()
    }

    pub fn clear(&mut self) {
        self.buf.fill(0);
        self.cursor = 0;
    }

    pub fn len(&self) -> usize {
        self.cursor
    }

    pub fn is_empty(&self) -> bool {
        self.cursor == 0
    }

    pub fn full(&self) -> bool {
        self.capacity() == self.cursor
    }

    pub fn to_string(&self) -> heapless::String<N> {
        use heapless::String;
        String::from_str(self.as_str()).unwrap()
    }
}

impl<const N: usize> core::borrow::Borrow<[u8]> for ByteMutWriterCap<N> {
    fn borrow(&self) -> &[u8] {
        &self.buf[0..self.cursor]
    }
}

impl<const N: usize> core::fmt::Write for ByteMutWriterCap<N> {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        let cap = self.capacity();
        for (i, &b) in self.buf[self.cursor..cap]
            .iter_mut()
            .zip(s.as_bytes().iter())
        {
            *i = b;
        }
        self.cursor = usize::min(cap, self.cursor + s.as_bytes().len());
        Ok(())
    }
}
