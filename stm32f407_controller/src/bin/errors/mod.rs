use defmt::Format;

#[derive(Debug, Format)]
pub enum StmError {
    InvalidConfigData,
    ConfigDeserializeError,
    FileNotFound,
    InvalidFileName,
    InvalidHttpRequest,
    InvalidIpDetails,
    InvalidDnsDetails,
    BadMqttIp,
    BadMqttPort,
}
impl core::error::Error for StmError {}
impl core::fmt::Display for StmError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            StmError::InvalidConfigData => write!(f, "Invalid config data"),
            StmError::ConfigDeserializeError => {
                write!(f, "Error deserializing config")
            }
            StmError::FileNotFound => write!(f, "File not found"),
            StmError::InvalidFileName => write!(f, "Bad filename"),
            StmError::InvalidHttpRequest => write!(f, "InvalidHttpRequest"),
            StmError::InvalidIpDetails => write!(f, "InvalidIpDetails"),
            StmError::InvalidDnsDetails => write!(f, "InvalidDnsDetails"),
            StmError::BadMqttIp => write!(f, "BadMqttIp"),
            StmError::BadMqttPort => write!(f, "BadMqttPort"),
        }
    }
}
