#![no_std]
#![allow(dead_code)]

/// Basic syslog client
/// To be implemented
use embassy_net::{tcp::TcpSocket, udp::UdpSocket, IpEndpoint};

pub enum SyslogSocket<'a> {
    Tcp(TcpSocket<'a>),
    Udp(UdpSocket<'a>, IpEndpoint), // was IpEndpoint - disapeared in update
}
struct Syslog<'a> {
    socket: SyslogSocket<'a>,
}

impl<'a> Syslog<'a> {
    fn new(socket: SyslogSocket<'a>) -> Self {
        Self { socket }
    }
    pub async fn log_info(&mut self, log_message: &str) -> Result<(), ()> {
        // format log_message into a Syslog compatible &str to be sent to the Syslog server
        // let data = format!("<{}>{} - {}", 5, log_message, Instant::now().to_secs());
        let data = log_message;
        self.transmit(data.as_bytes()).await
    }
    async fn transmit(&mut self, data: &[u8]) -> Result<(), ()> {
        let _ = match &mut self.socket {
            SyslogSocket::Tcp(socket) => socket.write(data).await.map(|_| ()).map_err(|_| ()),
            SyslogSocket::Udp(socket, ip) => socket.send_to(data, *ip).await.map_err(|_| ()),
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
