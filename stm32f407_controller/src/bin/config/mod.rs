use core::str::FromStr;

use crate::errors::StmError;
use alloc::string::ToString;
// use crate::tasks::ntp::Time;
use bms_standard::MinMax;
use defmt::error;
use miniserde::__private::String;
use miniserde::{json, Deserialize, Serialize};
use no_std_net::{Ipv4Addr, SocketAddr, SocketAddrV4};

#[derive(Default, Serialize)]
pub struct GlobalState {
    state: State,
    fault: Fault,
    time: i64,
}
impl GlobalState {
    pub fn state(&self) -> &State {
        &self.state
    }
    pub fn fault(&self) -> &Fault {
        &self.fault
    }
    pub fn set_state(&mut self, state: State) {
        self.state = state;
    }
    pub fn set_fault(&mut self, fault: Fault) {
        self.fault = fault;
    }
}

#[derive(Serialize, Deserialize, Debug)] // references only
pub struct Config {
    charge_current: MinMax<f32>,
    discharge_current: MinMax<f32>,
    current_sensor: MinMax<f32>,
    pack_volts: MinMax<f32>,
    cell_temperatures: MinMax<f32>,
    pack_temperatures: MinMax<f32>,
    cell_millivolt_peak: u16,
    cells_mv: MinMax<u16>,
    cell_millivolt_delta_max: u16,
    soc: MinMax<u8>,
    pub dod: MinMax<u8>,
}

#[derive(Serialize, Deserialize, Debug, Default)] // references only
pub struct NetConfig {
    pub dhcp: bool,
    pub ip: Option<String>,
    pub netmask: Option<String>,
    pub gateway: Option<String>,
    pub dns: Option<String>,
}

impl NetConfig {
    pub fn new(
        dhcp: bool,
        ip: Option<String>,
        netmask: Option<String>,
        gateway: Option<String>,
        dns: Option<String>,
    ) -> Self {
        Self {
            dhcp,
            ip,
            netmask,
            gateway,
            dns,
        }
    }
    pub fn get_config(&self) -> embassy_net::Config {
        if self.dhcp | self.ip.is_none() | self.netmask.is_none() {
            embassy_net::Config::dhcpv4(Default::default())
        } else {
            match self.create_config() {
                Ok(config) => config,
                Err(e) => {
                    error!("Config validitiy error {}", e);
                    embassy_net::Config::dhcpv4(Default::default())
                }
            }
        }
    }
    fn create_config(&self) -> Result<embassy_net::Config, StmError> {
        let ipaddress = embassy_net::Ipv4Address::from_str(self.ip.as_ref().unwrap())
            .map_err(|_| StmError::InvalidIpDetails)?;
        let address = embassy_net::Ipv4Cidr::new(ipaddress, 24);
        let mut dns_servers: heapless::Vec<embassy_net::Ipv4Address, 3> = heapless::Vec::new();
        if let Some(dns) = &self.dns {
            let dns =
                embassy_net::Ipv4Address::from_str(dns).map_err(|_| StmError::InvalidDnsDetails)?;
            dns_servers.push(dns).unwrap();
        };
        let gateway = if let Some(gw) = &self.gateway {
            match embassy_net::Ipv4Address::from_str(gw) {
                Ok(g) => Some(g),
                Err(_) => None,
            }
        } else {
            None
        };
        Ok(embassy_net::Config::ipv4_static(
            embassy_net::StaticConfigV4 {
                address,
                dns_servers,
                gateway,
            },
        ))
    }
}

#[derive(Default)]
pub struct MqttConfigBuilder {
    // Fields for the builder
    host: Option<String>,
    port: Option<u16>,
    client_id: Option<String>,
    username: Option<String>,
    password: Option<String>,
    basetopic: Option<String>,
    qos: u8,
    retain: bool,
    interval: u16,
}

impl MqttConfigBuilder {
    // Initialize a new builder
    pub fn new() -> Self {
        Self {
            interval: 10,
            ..Default::default()
        }
    }

    // Setter methods for each field
    pub fn host(mut self, host: &str) -> Self {
        self.host = Some(host.to_string());
        self
    }

    pub fn port(mut self, port: u16) -> Self {
        self.port = Some(port);
        self
    }

    pub fn client_id(mut self, client_id: &str) -> Self {
        self.client_id = Some(client_id.to_string());
        self
    }

    pub fn username(mut self, username: &str) -> Self {
        self.username = Some(username.to_string());
        self
    }

    pub fn password(mut self, password: &str) -> Self {
        self.password = Some(password.to_string());
        self
    }

    pub fn basetopic(mut self, basetopic: &str) -> Self {
        self.basetopic = Some(basetopic.to_string());
        self
    }

    pub fn qos(mut self, qos: u8) -> Self {
        self.qos = qos;
        self
    }

    pub fn retain(mut self, retain: bool) -> Self {
        self.retain = retain;
        self
    }

    pub fn interval(mut self, interval: u16) -> Self {
        self.interval = interval;
        self
    }

    // Build the MqttConfig
    pub fn build(self) -> MqttConfig {
        MqttConfig {
            host: self.host,
            port: self.port,
            client_id: self.client_id,
            username: self.username,
            password: self.password,
            basetopic: self.basetopic,
            qos: self.qos,
            retain: self.retain,
            interval: self.interval,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)] // references only
pub struct MqttConfig {
    host: Option<String>,
    port: Option<u16>,
    client_id: Option<String>,
    username: Option<String>,
    password: Option<String>,
    basetopic: Option<String>,
    pub qos: u8,
    pub retain: bool,
    pub interval: u16,
}

impl MqttConfig {
    pub fn builder() -> MqttConfigBuilder {
        MqttConfigBuilder::new()
    }

    pub fn get_config(&self) -> &MqttConfig {
        self
    }
    pub fn get_username(&self) -> &str {
        self.username.as_ref().unwrap()
    }
    pub fn get_password(&self) -> &str {
        self.password.as_ref().unwrap()
    }
    pub fn get_qos(&self) -> u8 {
        self.qos
    }
    pub fn get_retain(&self) -> bool {
        self.retain
    }
    pub fn get_interval(&self) -> u16 {
        self.interval
    }
    pub fn get_topic(&self) -> &str {
        self.basetopic.as_ref().unwrap()
    }
}

impl TryFrom<&MqttConfig> for SocketAddr {
    type Error = StmError;

    fn try_from(conf: &MqttConfig) -> Result<Self, Self::Error> {
        if conf.host.is_none() {
            return Err(StmError::BadMqttIp);
        };
        if conf.port.is_none() {
            return Err(StmError::BadMqttPort);
        }

        let ip =
            Ipv4Addr::from_str(conf.host.as_ref().unwrap()).map_err(|_| StmError::BadMqttIp)?;
        Ok(SocketAddr::V4(SocketAddrV4::new(ip, conf.port.unwrap())))
    }
}

/*
impl MqttConfig {
    pub fn new(
        host: Option<String>,
        port: Option<u16>,
        client_id: Option<String>,
        username: Option<String>,
        password: Option<String>,
        basetopic: Option<String>,
        qos: u8,
        retain: bool,
        interval: u16,
    ) -> Self {
        Self {
            name: ConfigName::MqttConfig,
            host,
            port,
            client_id,
            username,
            password,
            basetopic,
            qos,
            retain,
            interval,
        }
    }
    pub fn get_config(&self) -> &MqttConfig {
        self
    }
}
*/
pub trait JsonTrait
where
    Self: Serialize + Deserialize,
{
    fn decode_from_json(&mut self, slice: &[u8]) -> Result<(), StmError> {
        *self = json::from_str::<Self>(
            core::str::from_utf8(slice).map_err(|_e| StmError::InvalidConfigData)?,
        )
        .map_err(|_e| StmError::ConfigDeserializeError)?;
        Ok(())
    }

    fn to_json(&self) -> String {
        json::to_string(&self)
    }
}

impl JsonTrait for Config {}
impl JsonTrait for MqttConfig {}
impl JsonTrait for NetConfig {}

impl Config {
    pub fn pack_volts(&self) -> &MinMax<f32> {
        &self.pack_volts
    }

    pub fn export_as_bms(&self) -> bms_standard::Config {
        self.into()
    }
    pub fn import_from_bms(&mut self, bms_config: bms_standard::Config) {
        self.charge_current = *bms_config.charge_current_limts();
        self.discharge_current = *bms_config.discharge_current_limts();
        self.current_sensor = *bms_config.current_sensor_limts();
        self.pack_volts = *bms_config.pack_volts();
        self.cell_temperatures = *bms_config.pack_temperatures();
        self.pack_temperatures = *bms_config.pack_temperatures();
        self.cell_millivolt_peak = bms_config.cell_millivolt_peak();
        self.cells_mv = *bms_config.cells_mv();
        self.cell_millivolt_delta_max = bms_config.cell_millivolt_delta_max();
        self.soc = *bms_config.soc();
    }
}
impl From<&Config> for bms_standard::Config {
    fn from(val: &Config) -> Self {
        bms_standard::Config {
            charge_current: val.charge_current,
            discharge_current: val.discharge_current,
            current_sensor: val.current_sensor,
            pack_volts: val.pack_volts,
            cell_temperatures: val.cell_temperatures,
            pack_temperatures: val.pack_temperatures,
            cell_millivolt_peak: val.cell_millivolt_peak,
            cells_mv: val.cells_mv,
            cell_millivolt_delta_max: val.cell_millivolt_delta_max,
            soc: val.soc,
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            charge_current: MinMax::new(0.0, 200.0),
            discharge_current: MinMax::new(0.0, 200.0),
            current_sensor: MinMax::new(-40.0, 40.0),
            pack_volts: MinMax::new(300.0, 400.0),
            cell_temperatures: MinMax::new(-20.0, 50.0),
            pack_temperatures: MinMax::new(-20.0, 50.0),
            cell_millivolt_peak: 4200,
            cells_mv: MinMax::new(3000, 4150),
            cell_millivolt_delta_max: 500,
            soc: MinMax::new(0, 100),
            dod: MinMax::new(0, 90),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Default)]
pub enum State {
    Online,
    #[default]
    Offline,
}
#[derive(Serialize, Deserialize, Debug, Default)]
pub enum Fault {
    InvFault,
    BmsFault,
    #[default]
    None,
}
