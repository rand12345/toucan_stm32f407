use super::mqtt::MqttFormat;
use crate::statics::*;
use bms_standard::Bms;
use defmt::{error, warn};
use embassy_stm32::can::bxcan::Frame;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex as _Mutex;
use embassy_sync::signal::Signal;
use ze40_bms::{can_frames::*, *};

pub static _CHARGING_STATUS: Signal<_Mutex, ChargingState> = Signal::new();
static LBC_STATUS: Signal<_Mutex, LbcKey> = Signal::new();

const PREAMBLE_TIME_MS: u64 = 95;
const DIAG_TIME_MS: u64 = 5000;

#[cfg(feature = "ze40")]
#[embassy_executor::task]
pub async fn bms_tx_periodic() {
    use embassy_futures::select::{select, Either};
    use embassy_time::{Duration, Ticker};
    let tx = BMS_CHANNEL_TX.sender();
    let ticker_ms = |ms| Ticker::every(Duration::from_millis(ms));
    let sender = |frame| {
        if let Err(_e) = tx.try_send(frame) {
            error!("BMS: Periodic queue buf error: {}", _e)
        };
    };

    warn!("Starting BMS TX periodic");
    let mut t1 = ticker_ms(PREAMBLE_TIME_MS);
    let mut t2 = ticker_ms(DIAG_TIME_MS);
    let mut lbc_key: Option<LbcKey> = None;
    let mut counter = 0;
    loop {
        match select(t1.next(), t2.next()).await {
            Either::First(_) => {
                sender(request_frame(ChargingState::Charging, &lbc_key).unwrap());
                counter += 1;
                lbc_key = match counter {
                    1..=5 => Some(LbcKey::X5d),
                    6..=9 => Some(LbcKey::Xb2),
                    _ => {
                        counter = 0;
                        Some(LbcKey::Xb2)
                    }
                };
            }
            Either::Second(_) => sender(request_tx_frame(RequestMode::CellBank1).unwrap()),
        }
    }
}

#[allow(unused_assignments)]
#[cfg(feature = "ze40")]
#[embassy_executor::task]
pub async fn bms_rx() {
    use bms_standard::BmsError;
    use embassy_stm32::can::bxcan::Id;
    use embassy_stm32::can::bxcan::Id::Standard;
    use embassy_time::Instant;

    let (mut f55, mut faa) = (0u8, 0u8);

    let rx = BMS_CHANNEL_RX.receiver();
    let tx = BMS_CHANNEL_TX.sender();
    let mut data = ze40_bms::Data::new();
    warn!("Starting ZE40 Rx Processor");
    let canid = |frame: &Frame| -> Option<u16> {
        match frame.id() {
            Standard(id) => Some(id.as_raw()),
            Id::Extended(_) => None,
        }
    };
    loop {
        let frame = rx.receive().await;
        // Process 10ms data
        let id = match canid(&frame) {
            Some(id) => id,
            None => continue,
        };
        if ![0x155, 0x424, 0x425, 0x4ae, 0x7bb, 0x445].contains(&id) {
            continue; // filter unwanted frames
        }

        if id == 0x445 {
            x445_signal(frame, &mut faa, &mut f55);
            continue;
        };
        if id != 1979 {
            let update_inverter = match data.rapid_data_processor(frame) {
                Ok(state) => state,
                Err(e) => {
                    warn!("Rapid data parsing error: {}", e);
                    continue;
                }
            };
            if update_inverter {
                // change to BMS wdt and signal update
                {
                    *LAST_BMS_MESSAGE.lock().await = Some(Instant::now());
                }
                let mut bmsdata = BMS.lock().await;
                let mut update = || -> Result<(), BmsError> {
                    WDT.signal(true); // temp whilst testing
                    defmt::debug!(
                        "Data: Current: {}A SoC: {}% Remaining: {}kWh Charge Rate: {}maxA Pack: {}ºC",
                        data.current_value,
                        data.soc_value,
                        data.kwh_remaining,
                        data.max_charge_amps,
                        data.pack_temp
                    );
                    bmsdata
                        .set_valid(false)?
                        .set_current(data.current_value)?
                        .set_soc(data.soc_value)?
                        .set_kwh(data.kwh_remaining)?
                        .set_pack_temp(data.pack_temp)?
                        .set_valid(true)?;
                    Ok(())
                };
                if let Err(e) = update() {
                    error!("Rapid data update error: {}", e)
                } else {
                    // push_all_to_mqtt(bmsdata).await;
                };
            }
        } else {
            match data.diag_data_processor(frame) {
                Ok(None) => {
                    WDT.signal(true); // temp whilst testing
                    {
                        *LAST_BMS_MESSAGE.lock().await = Some(Instant::now());
                    }
                    let mut bmsdata = BMS.lock().await;
                    // update_dod(&mut bmsdata).await;
                    let mut update = || -> Result<(), BmsError> {
                        bmsdata.bal_cells = data.bal_cells.0;

                        defmt::debug!(
                            "Data: Cell Range H/L: {}mV {}mV Pack Volts: {}V Temperatures H/L: {}ºC {}ºC",
                            data.cell_mv.maximum(),
                            data.cell_mv.minimum(),
                            data.pack_volts,
                            data.temp.maximum(),
                            data.temp.minimum(),
                        );
                        bmsdata.cell_range_mv = data.cell_mv;
                        bmsdata.pack_volts = data.pack_volts;

                        bmsdata
                            .set_valid(false)?
                            .set_max_discharge_amps(35.0)?
                            .set_cell_mv(data.cells_mv)?
                            .set_pack_volts(data.pack_volts)?
                            .set_temps(*data.temp.minimum(), *data.temp.maximum())?
                            .set_max_charge_amps(data.max_charge_amps)?
                            .throttle_pack()?
                            .set_valid(true)?;
                        if bmsdata.get_balancing_cells() > 0 {
                            bmsdata.debug_balancing_cells()
                        }
                        bmsdata.discharge_max = 35.0;
                        defmt::debug!(
                            "Data: Charge max: {}A Discharge max: {}A Shunts: {} SoC {}",
                            bmsdata.charge_max,
                            bmsdata.discharge_max,
                            bmsdata.get_balancing_cells(),
                            bmsdata.soc
                        );
                        push_all_to_mqtt(*bmsdata);
                        Ok(())
                    };
                    if let Err(e) = update() {
                        error!("Diag update error: {}", e)
                    };
                }

                Ok(Some(next_tx_frame)) => {
                    tx.send(next_tx_frame).await;
                    continue;
                }
                Err(e) => {
                    error!("BMS diag error: {:?}", e);
                }
            };
        }
    }
}

fn x445_signal(frame: Frame, faa: &mut u8, f55: &mut u8) {
    let lbc_key = match frame.data() {
        Some(data) => data[2],
        None => return,
    };

    match lbc_key {
        0xaa => {
            *f55 = 0;
            *faa += 1;
            if faa == &4 {
                *faa = 0;
                LBC_STATUS.signal(LbcKey::X5d);
            }
        }
        _ => {
            *faa = 0;
            *f55 += 1;
            if f55 == &4 {
                *f55 = 0;
                LBC_STATUS.signal(LbcKey::Xb2);
            }
        }
    };
}

#[inline]
fn push_all_to_mqtt(bms: Bms) {
    // let bms = BMS.lock().await;
    if let Ok(mut lock) = MQTTFMT.try_lock() {
        *lock = MqttFormat::from(bms)
    };
}
