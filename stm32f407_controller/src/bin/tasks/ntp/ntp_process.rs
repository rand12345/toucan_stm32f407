use crate::types::StackType;
use chrono::{DateTime as ChronoDateTime, NaiveDateTime, TimeZone};
use chrono_tz::Tz;
use core::str::FromStr;
use defmt::{error, info, warn, Debug2Format, Format};
use embassy_net::{
    udp::{PacketMetadata, UdpSocket},
    IpAddress, IpEndpoint, Ipv4Address,
};
use embassy_stm32::rtc::{DateTime, Rtc};
use no_std_net::{SocketAddr, ToSocketAddrs};
use sntpc::{NtpContext, NtpResult, NtpTimestampGenerator};

include!("timezone.rs");

#[derive(Debug, Format)]
pub enum NtpError {
    Parse,
}
impl core::error::Error for NtpError {}
impl core::fmt::Display for NtpError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Parse => write!(f, "Parse"),
        }
    }
}

///Seconds
#[derive(Format)]
struct EpochTime(i64);

impl EpochTime {
    fn get_datetime(&self) -> Option<NaiveDateTime> {
        NaiveDateTime::from_timestamp_opt(self.0, 0)
    }
}

impl From<Time> for EpochTime {
    fn from(value: Time) -> Self {
        Self(value.0.timestamp())
    }
}

#[derive(Debug)]
pub struct Time(ChronoDateTime<Tz>);

impl TryFrom<EpochTime> for Time {
    type Error = NtpError;
    fn try_from(epoch: EpochTime) -> Result<Self, Self::Error> {
        epoch.get_datetime().ok_or(NtpError::Parse).and_then(|nd| {
            TZ.from_local_datetime(&nd)
                .single()
                .ok_or(NtpError::Parse)
                .map(Time)
        })
    }
}

impl TryFrom<NtpResult> for Time {
    type Error = NtpError;
    fn try_from(time: NtpResult) -> Result<Self, Self::Error> {
        EpochTime(time.sec().into()).try_into()
    }
}

#[cfg(feature = "ntp")]
#[allow(unused_mut)]
#[embassy_executor::task]
pub async fn ntp_task(_stack: StackType, mut rtc: Rtc) {
    // use chrono::Utc;
    use crate::statics::UTC_NOW;
    info!("Launching RTC");

    // #[cfg(feature = "no_net")]
    // {
    //     let rtc_tmp: DateTime = chrono::NaiveDateTime::from_timestamp_opt(1711529060, 0)
    //         .unwrap()
    //         .into();
    //     rtc.set_datetime(rtc_tmp).expect("Set RTC to 2024 failed");
    // }

    #[cfg(not(feature = "no_net"))]
    if env!("NTPSERVER").is_empty() {
        warn!("No NTP server configured in .env");
    } else {
        // let rtc_now = rtc.now().unwrap_or(NaiveDateTime::MIN.into());
        let rtc_now: DateTime = rtc.now().unwrap_or(
            chrono::NaiveDateTime::from_timestamp_opt(1711529060, 0)
                .unwrap()
                .into(),
        );
        let ntp_cfg_ip = env!("NTPSERVER");
        info!("Spawning Network NTP client");

        let mut rx_buffer = [0; 512];
        let mut tx_buffer = [0; 512];
        let mut rx_meta = [PacketMetadata::EMPTY; 16];
        let mut tx_meta = [PacketMetadata::EMPTY; 16];
        let socket = UdpSocket::new(
            _stack,
            &mut rx_meta,
            &mut rx_buffer,
            &mut tx_meta,
            &mut tx_buffer,
        );
        let stddt = StdTimestampGen::from(rtc_now);
        warn!("Sending this to NTP: {:?}", Debug2Format(&stddt.datetime));
        let ntp_context = NtpContext::new(stddt);

        let endpoint = {
            let (ntp_ip, port) = ntp_cfg_ip.rsplit_once(':').expect("Bad NTP ip:port");
            let ntp_server_address =
                Ipv4Address::from_str(ntp_ip).expect("Invalid NTP server IP address");
            let ntp_port = port.parse::<u16>().expect("Invalid NTP server port");

            IpEndpoint::new(IpAddress::Ipv4(ntp_server_address), ntp_port)
        };

        let ntpsocket =
            UdpSocketWrapper(NoStdUdpSocket::bind(socket, endpoint).expect("NTP UDP Bind fail"));

        let result = sntpc::get_time(ntpsocket.0.socketaddress, &ntpsocket, ntp_context).await;
        let new_rtc_time: Option<NaiveDateTime> = match result {
            Ok(time) => match Time::try_from(time) {
                Ok(local_time) => EpochTime::from(local_time).get_datetime(),
                Err(_) => {
                    error!("NTP time conversion failed with parse error");
                    None
                }
            },
            Err(err) => {
                error!("NTP update failed with {:?}", Debug2Format(&err));
                None
            }
        };

        if let Some(ntp_time) = new_rtc_time {
            let _ = rtc.set_datetime(ntp_time.into());
            print_time(&rtc);
        }
    };

    let mut tick = embassy_time::Ticker::every(embassy_time::Duration::from_secs(1));
    info!("RTC update loop");
    loop {
        // ("%Y-%m-%dT%H:%M:%S.%fZ"
        tick.next().await;
        if let Ok(now) = rtc.now() {
            UTC_NOW.signal(now)
        };
        // embassy_time::Timer::after(embassy_time::Duration::from_secs(60 * 60)).await;

        print_time(&rtc);
        // try update call here
    }
}

fn print_time(rtc: &Rtc) {
    if let Ok(now) = rtc.now() {
        info!(
            "RTC: {}/{}/{} {}:{}:{}",
            now.year(),
            now.month(),
            now.day(),
            now.hour(),
            now.minute(),
            now.second(),
        );
    }
}

#[derive(Copy, Clone, Debug)]
struct StdTimestampGen {
    pub datetime: NaiveDateTime,
}

impl From<DateTime> for StdTimestampGen {
    fn from(rtc: DateTime) -> Self {
        Self {
            datetime: rtc.into(),
        }
    }
}

impl NtpTimestampGenerator for StdTimestampGen {
    fn init(&mut self) {}
    fn timestamp_sec(&self) -> u64 {
        warn!("timestamp_sec: {}", self.datetime.timestamp() as u64);
        self.datetime.timestamp() as u64
    }
    fn timestamp_subsec_micros(&self) -> u32 {
        warn!(
            "timestamp_subsec_micros: {}",
            self.datetime.timestamp_micros() as u32
        );
        self.datetime.timestamp_micros() as u32
    }
}

struct NoStdUdpSocket<'a> {
    socket: UdpSocket<'a>,
    socketaddress: SocketAddr,
    endpoint: IpEndpoint,
}

impl core::fmt::Debug for NoStdUdpSocket<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.socketaddress)
    }
}

impl<'a> NoStdUdpSocket<'a> {
    fn bind(mut socket: UdpSocket<'a>, endpoint: IpEndpoint) -> sntpc::Result<Self> {
        let ip: [u8; 4] = endpoint
            .addr
            .as_bytes()
            .try_into()
            .map_err(|_| sntpc::Error::IpDecode)?;
        let socketaddress = SocketAddr::new(
            no_std_net::IpAddr::V4(no_std_net::Ipv4Addr::from(ip)),
            endpoint.port,
        );

        socket.bind(socketaddress.port()).map_err(|e| {
            defmt::error!("NTP Bind fail {}", e);
            sntpc::Error::NetworkBind
        })?;

        Ok(Self {
            socket,
            socketaddress,
            endpoint,
        })
    }

    async fn send_to<T: ToSocketAddrs>(&self, buf: &[u8], _dest: T) -> sntpc::Result<usize> {
        self.socket
            .send_to(buf, self.endpoint)
            .await
            .map_err(|_| sntpc::Error::SendError)?;
        Ok(buf.len())
    }
    async fn recv_from(&self, buf: &mut [u8]) -> sntpc::Result<(usize, SocketAddr)> {
        let (size, _) = self
            .socket
            .recv_from(buf)
            .await
            .map_err(|_| sntpc::Error::ReceiveError)?;
        Ok((size, self.socketaddress))
    }
}

#[derive(Debug)]
struct UdpSocketWrapper<'a>(NoStdUdpSocket<'a>);

impl<'a> sntpc::NtpUdpSocket<'a> for UdpSocketWrapper<'a> {
    async fn send_to<T: ToSocketAddrs>(&self, buf: &[u8], addr: T) -> sntpc::Result<usize> {
        self.0.send_to(buf, addr).await
    }

    async fn recv_from(&self, buf: &mut [u8]) -> sntpc::Result<(usize, SocketAddr)> {
        self.0.recv_from(buf).await
    }
}
