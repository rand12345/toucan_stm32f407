use super::{models::*, ModbusError};
use crate::statics::LED_COMMAND;
use crate::tasks::modbus::conversions::*;

use crate::tasks::leds::Led::Led3;
use crate::tasks::leds::LedCommand::{Off, On};

// fn dummy_tcp_request_filter(input: TcpRequestPayload) -> TcpRequestPayload {
//     input
// }

#[cfg(all(feature = "modbus_client", feature = "OB737"))]
pub async fn process_flow(
    modbus_tcp: &mut ModbusTcp<'_>,
    modbus_rtu: &mut ModbusRtu<'_>,
) -> Result<(), ModbusError> {
    use crate::tasks::modbus::flow::ob737::FoxOb737;

    LED_COMMAND.signal(Off(Led3));
    let mut ob737 = FoxOb737::default();
    modbus_rtu
        .listen() // rx in data.0, performs crc check
        .await?
        .convert_request(convert_rtu_request_payload_to_tcp_request_payload) // Add TCP filter
        .map(|f| ob737.fox_to_om737_tcp_request_filter(f))?
        .map(|tcp_payload| modbus_tcp.send_and_receive(tcp_payload))?
        .await?
        .convert_response(convert_tcp_reponse_payload_to_rtu_response_payload) // Add RTU data filter
        .map(|f| ob737.om737_to_fox_rtu_request_filter(f))?
        .map(|rtu_send| modbus_rtu.send(rtu_send))?
        .await?;

    LED_COMMAND.signal(On(Led3));
    Ok(())
}

#[cfg(all(feature = "modbus_client", not(feature = "OB737")))]
pub async fn process_flow(
    modbus_tcp: &mut ModbusTcp<'_>,
    modbus_rtu: &mut ModbusRtu<'_>,
) -> Result<(), ModbusError> {
    LED_COMMAND.signal(Off(Led3));
    modbus_rtu
        .listen() // rx in data.0, performs crc check
        .await?
        .convert_request(convert_rtu_request_payload_to_tcp_request_payload) // middleware glue section
        .map(|tcp_payload| modbus_tcp.send_and_receive(tcp_payload))?
        .await?
        .convert_response(convert_tcp_reponse_payload_to_rtu_response_payload)
        .map(|rtu_send| modbus_rtu.send(rtu_send))?
        .await?;

    LED_COMMAND.signal(On(Led3));
    Ok(())
}

#[cfg(feature = "modbus_bridge")]
pub async fn process_flow(
    modbus_tcp: &mut ModbusTcp<'_>,
    modbus_rtu: &mut ModbusRtu<'_>,
) -> Result<(), ModbusError> {
    LED_COMMAND.signal(Off(Led3));
    modbus_tcp
        .listen() // rx in data.0, performs crc check
        .await?
        .convert_request(convert_tcp_request_payload_to_rtu_request_payload)
        .map(|rtu_req| modbus_rtu.send_and_receive(rtu_req))?
        .await? // middleware map section
        .convert_response(convert_rtu_reponse_payload_to_tcp_response_payload)
        .map(|tcp_send| modbus_tcp.send(tcp_send))?
        .await?;

    LED_COMMAND.signal(On(Led3));
    Ok(())
}

#[cfg(feature = "ob737")]
mod ob737 {
    use super::*;

    #[derive(Default)]
    pub struct FoxOb737 {
        request: TcpRequestPayload,
        meters: [f32; 3],
        reactive: u16,
        read_first: bool,
    }
    impl FoxOb737 {
        pub fn fox_to_om737_tcp_request_filter(
            &mut self,
            input: TcpRequestPayload,
        ) -> Result<TcpRequestPayload, ModbusError> {
            //check request
            self.request = input;
            const FOX_REQ: [u8; 2] = [1, 3];
            let (mbap, pdu) = self.request.split_at(6);
            if FOX_REQ != pdu[..2] {
                return Err(ModbusError::InvalidFilter);
            }

            // Will require two requests to get full dataset of powers and reactive
            // this is due to the spread of modbus registers, they cannot all be called in
            // one request.
            // Data requests from Fox are high frequency so this should not be a problem.
            let pl_request = if self.read_first {
                &[0, 0, 0, 6, 1, 4, 0, 144, 0, 6]
            } else {
                &[0, 0, 0, 6, 1, 4, 1, 16, 0, 8]
            };
            self.read_first = !self.read_first; // toggle

            //create new request (as function pair)
            let mut output = TcpRequestPayload::from_slice(&mbap[..2]).unwrap();
            output.extend_from_slice(pl_request).unwrap();
            assert!(output.len() == 12);
            Ok(output)
        }

        pub fn om737_to_fox_rtu_request_filter(
            &mut self,
            input: ResponsePayload,
        ) -> Result<ResponsePayload, ModbusError> {
            //parse rtu response
            // 01 04 10 3d 13 74 bc 3f 41 89 37 3e b0 20 c5 3f 90 a3 d7 61 03
            let bytes_len = input[2];
            match bytes_len {
                12 => {
                    //decode phases (energy)
                    defmt::info!("Parsing OB737 energy meter");
                    let response_data = &input[3..];
                    // Parse the watt meters into f32 registers
                    response_data
                        .chunks(4)
                        .enumerate()
                        .take(3)
                        .for_each(|(u, v)| {
                            self.meters[u] = f32::from_be_bytes([v[0], v[1], v[2], v[3]]) * 10000.0;
                            defmt::info!("Phase {} : {}W", u + 1, self.meters[u]);
                        });
                }
                16 => {
                    //decode reactive total(?)
                    let response_data = &input[3..];
                    // historically, 3 phases were taken along with the total
                    response_data.chunks(4).skip(3).take(1).for_each(|v| {
                        self.reactive = f32::from_be_bytes([v[0], v[1], v[2], v[3]]) as u16;
                        defmt::info!("Reactive {}", self.reactive);
                    });
                }
                _ => {
                    defmt::error!("Response from OM737 is unexpected: {:02x}", input);
                    return Ok(input);
                }
            }

            let mut payload = ResponsePayload::new();
            let _ = payload.extend_from_slice(&self.request[0..2]);
            let _ = payload.push(self.request[5]); // len
            let _ = payload.extend_from_slice(&self.meters[0].to_be_bytes());
            let _ = payload.extend_from_slice(&self.meters[1].to_be_bytes());
            let _ = payload.extend_from_slice(&self.meters[2].to_be_bytes());
            let _ = payload.extend_from_slice(&self.reactive.to_be_bytes());
            let _ = payload.extend_from_slice(&create_crc(&payload[0..17]));

            Ok(payload)
        }
    }
}
