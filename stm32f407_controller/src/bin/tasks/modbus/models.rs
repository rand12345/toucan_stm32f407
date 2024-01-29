use embassy_net::{IpEndpoint, Ipv4Address};
use no_std_net::SocketAddr;

#[cfg(feature = "modbus_client")]
use core::sync::atomic::{AtomicU16, Ordering};

#[cfg(feature = "modbus_client")]
static COUNTER: AtomicU16 = AtomicU16::new(0);

#[cfg(feature = "modbus_client")]
pub fn increment_counter() -> u16 {
    loop {
        let current = COUNTER.load(Ordering::Relaxed);
        if current == u16::MAX {
            COUNTER.store(0, Ordering::Relaxed);
            return 0;
        } else {
            let new = current + 1;
            match COUNTER.compare_exchange(current, new, Ordering::Relaxed, Ordering::Relaxed) {
                Ok(_) => return new,
                Err(_) => continue,
            }
        }
    }
}
#[cfg(feature = "modbus_client")]
pub fn get_counter_value() -> u16 {
    COUNTER.load(Ordering::Relaxed)
}

#[derive(Copy, Clone)]
pub enum ModbusMode {
    // Client ip:port
    Client(SocketAddr),
    // local port
    Server(u16),
}
impl ModbusMode {
    pub fn get_port(&self) -> u16 {
        match self {
            ModbusMode::Client(socket) => match socket {
                SocketAddr::V4(ip) => ip.port(),
                SocketAddr::V6(ip) => ip.port(),
            },
            ModbusMode::Server(p) => *p,
        }
    }
    pub fn get_ip(&self) -> Option<&SocketAddr> {
        match self {
            ModbusMode::Client(socket) => Some(socket),
            ModbusMode::Server(_) => None,
        }
    }
}
impl From<ModbusMode> for IpEndpoint {
    fn from(val: ModbusMode) -> Self {
        match val {
            ModbusMode::Client(socket) => match socket {
                SocketAddr::V4(ip) => {
                    IpEndpoint::from((Ipv4Address::from_bytes(&ip.ip().octets()), ip.port()))
                }
                SocketAddr::V6(_ip) => {
                    todo!()
                }
            },
            ModbusMode::Server(p) => IpEndpoint::from((Ipv4Address::default(), p)),
        }
    }
}
