// #![allow(dead_code)]
use super::{models::*, ModbusError};

const MODBUS_PROTO: [u8; 2] = [0, 0];

#[cfg(feature = "modbus_client")]
pub fn convert_rtu_request_payload_to_tcp_request_payload(
    input: &RtuRequestPayload,
) -> Result<TcpRequestPayload, ModbusError> {
    let mut output = TcpRequestPayload::default();
    if input.len() < 4 {
        defmt::error!("Payload too short");
        return Err(ModbusError::RtuPayloadTooShort);
    }
    let new_transaction_id = increment_transaction_id().to_be_bytes();
    let (payload, _crc) = input.split_at(input.len() - 2); //drop crc
    let bytes_length = (payload.len() as u16).to_be_bytes();

    output.extend(new_transaction_id);
    output.extend(MODBUS_PROTO); // modbus
    output.extend(bytes_length);
    output
        .extend_from_slice(payload)
        .map_err(|_| ModbusError::ConversionSlice)?;

    Ok(output)
}

#[cfg(feature = "modbus_client")]
pub fn convert_tcp_reponse_payload_to_rtu_response_payload(
    input: &ResponsePayload,
) -> Result<ResponsePayload, ModbusError> {
    if input.len() < 7 {
        defmt::error!("Payload too short");
        return Err(ModbusError::TcpRxFail(input.len()));
    }
    // copy bytes
    let (mbap, pdu) = input.split_at(6);
    // check transaction ID is current
    let mut output = ResponsePayload::default();

    let rx_id = u16::from_be_bytes([mbap[0], mbap[1]]);
    if get_transaction_id() != rx_id {
        defmt::error!("Counter value mismatch");
        return Err(ModbusError::InvalidTransactionId);
    }
    let len: usize = input[5].into();
    let pdu = &pdu[..len]; // limit to reported bytes
    output
        .extend_from_slice(pdu)
        .and(output.extend_from_slice(&create_crc(pdu)))
        .map_err(|_| ModbusError::ConversionSlice)?;
    Ok(output)
}
#[cfg(feature = "modbus_bridge")]
pub fn convert_tcp_request_payload_to_rtu_request_payload(
    input: &TcpRequestPayload,
) -> Result<RtuRequestPayload, ModbusError> {
    let mut output = RtuRequestPayload::default();
    if input.len() < 7 {
        return Err(ModbusError::TcpRxFail(input.len()));
    }
    let (mbap, pdu) = input.split_at(6);
    set_transaction_id(u16::from_be_bytes([mbap[0], mbap[1]]));
    output
        .extend_from_slice(pdu)
        .and(output.extend_from_slice(&create_crc(pdu)))
        .map_err(|_| ModbusError::ConversionSlice)?;
    Ok(output)
}

#[cfg(feature = "modbus_bridge")]
pub fn convert_rtu_reponse_payload_to_tcp_response_payload(
    input: &ResponsePayload,
) -> Result<ResponsePayload, ModbusError> {
    let mut output = ResponsePayload::default();
    let transaction_id = get_transaction_id().to_be_bytes();
    let (payload, _crc) = input.split_at(input.len() - 2); //drop crc
    let bytes_length = (payload.len() as u16).to_be_bytes();

    output.extend(transaction_id);
    output.extend(MODBUS_PROTO);
    output.extend(bytes_length);
    output
        .extend_from_slice(payload)
        .map_err(|_| ModbusError::ConversionSlice)?;

    Ok(output)
}
