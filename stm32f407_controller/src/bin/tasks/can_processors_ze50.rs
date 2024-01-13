use crate::statics::*;
use defmt::{error, info, warn, Debug2Format};
use embassy_sync::blocking_mutex::raw::ThreadModeRawMutex as _Mutex;
use embassy_sync::mutex::Mutex;
use embassy_time::Instant;
use lazy_static::lazy_static;

pub type MutexData = Mutex<_Mutex, ze50_bms::Data>;

lazy_static! {
    pub static ref ZE50_DATA: MutexData =
        embassy_sync::mutex::Mutex::new(ze50_bms::Data::default());
}

async fn update() {
    // push vals to ZE50_BMS struct when reading loop has finished
    let data = ZE50_DATA.lock().await;
    let mut bmsdata = BMS.lock().await;
    {
        WDT.signal(true); // temp whilst testing
        {
            *LAST_BMS_MESSAGE.lock().await = Some(Instant::now());
        }
        let mut update = || -> Result<(), bms_standard::BmsError> {
            let _soc = data.soc_value;

            // Calculate soc from cell millivolts
            #[cfg(feature = "v65")]
            let _soc = map_cellv_soc(data.v_high_cell);

            let max_charge_amps = *bmsdata.config.charge_current_limts().maximum();

            defmt::debug!(
                "Data: Cell Range H/L: {}mV {}mV Pack Volts: {}V",
                data.v_high_cell,
                data.v_low_cell,
                data.pack_volts
            );
            defmt::debug!("Data: Current {}A", bmsdata.current);
            defmt::debug!(
                "Data: Temperatures H/L: {}ºC {}ºC",
                data.temp_max,
                data.temp_min
            );

            bmsdata
                .set_valid(false)?
                .set_soc(_soc)?
                .set_cell_mv_low_high(data.v_low_cell, data.v_high_cell)?
                .set_pack_volts(data.pack_volts)?
                .set_current(data.current_value)?
                .set_temps(data.temp_min, data.temp_max)?
                .set_kwh(data.kwh_remaining)?
                .set_max_charge_amps(max_charge_amps)?
                .set_pack_temp(data.pack_temp)?
                .throttle_pack()? //Adjusts current ch/dis based on conditions
                .set_valid(true)?;

            defmt::debug!(
                "Data: Charge max: {}A Discharge max: {}A Shunts: {} SoC {}",
                bmsdata.charge_max,
                bmsdata.discharge_max,
                bmsdata.get_balancing_cells(),
                _soc
            );
            #[cfg(feature = "mqtt")]
            push_all_to_mqtt(*bmsdata);
            Ok(())
        };
        if let Err(e) = update() {
            error!("Diag update error: {}", e)
        };
        info!("ZE50 Debug {}", Debug2Format(&*bmsdata));
    };
}

#[cfg(feature = "ze50")]
#[embassy_executor::task]
pub async fn bms_tx_periodic() {
    use embassy_futures::select::{select, Either};
    use embassy_stm32::can::bxcan::Frame;
    use embedded_hal::can::Frame as _;
    use embedded_hal::can::{ExtendedId, Id, StandardId};

    use embassy_time::{Duration, Ticker};
    use ze50_bms::{init_payloads, preamble_payloads};

    let tx = BMS_CHANNEL_TX.sender();
    let ticker_ms = |ms| Ticker::every(Duration::from_millis(ms));
    let sender = |frame| {
        if let Err(_e) = tx.try_send(frame) {
            error!("Periodic queue buf error")
        };
    };

    ticker_ms(2000).next().await;
    let mut preamble_frame_number_1 = true;
    let preamble_payloads = preamble_payloads();
    warn!("Starting BMS TX periodic");

    // send init
    for payload in init_payloads() {
        ticker_ms(200).next().await;
        let frame = Frame::new(Id::Standard(StandardId::new(0x373).unwrap()), &payload);
        sender(frame.unwrap());
    }

    let mut t1 = ticker_ms(200);
    let mut t2 = ticker_ms(225);
    loop {
        let frame: Frame = match select(t1.next(), t2.next()).await {
            Either::First(_) => {
                let payload = {
                    if preamble_frame_number_1 {
                        preamble_frame_number_1 = false;
                        preamble_payloads[0]
                    } else {
                        preamble_frame_number_1 = true;
                        preamble_payloads[1]
                    }
                };
                Frame::new(Id::Standard(StandardId::new(0x373).unwrap()), &payload).unwrap()
            }
            Either::Second(_) => {
                // read the current ZE50_DATA mode
                let pid = { ZE50_DATA.lock().await.req_code };
                if pid == 0x1 {
                    // end of reading loop
                    update().await
                }
                let pid_id = if pid == 0x5d {
                    // current 100ms sampling
                    0x92
                } else if pid == 0xc8 {
                    // kWh remaining
                    0x91
                } else {
                    // majority of readings
                    0x90
                };
                Frame::new(
                    Id::Extended(ExtendedId::new(0x18DADBF1).unwrap()),
                    &[0x03, 0x22, pid_id, pid, 0xff, 0xff, 0xff, 0xff],
                )
                .unwrap()
            }
        };
        sender(frame);
    }
}

#[cfg(feature = "ze50")]
#[allow(unused_assignments)]
#[embassy_executor::task]
pub async fn bms_rx() {
    use defmt::info;
    use embassy_stm32::can::bxcan::{Frame, Id::Extended};
    let rx = BMS_CHANNEL_RX.receiver();
    warn!("Starting ZE50 RX");
    loop {
        let frame: Frame = rx.receive().await;
        if let Extended(id) = frame.id() {
            if id.as_raw() == !0x18DAF1DB {
                info!("Unknown Extended ID - RX: {:02x}", id.as_raw());
                continue;
            }
            if frame.data().is_none() {
                continue;
            }
            let mut data = ZE50_DATA.lock().await;
            // process_payload into Data struct
            if let Err(e) = data.process_payload(frame.data().unwrap()) {
                defmt::error!("ZE50 process_payload {:?}", Debug2Format(&e));
            } else {
                *LAST_BMS_MESSAGE.lock().await = Some(Instant::now());
                // info!("BMS last message time reset")
            };
        } else {
            error!("Found standard Id on ZE50 can line - check filter");
        }
    }
}

#[cfg(feature = "mqtt")]
#[inline]
fn push_all_to_mqtt(bms: Bms) {
    // let bms = BMS.lock().await;
    if let Ok(mut lock) = MQTTFMT.try_lock() {
        *lock = bms.into()
    };
}

#[cfg(feature = "v65")]
#[inline]
fn map_cellv_soc(value: impl Into<f32>) -> f32 {
    // specify voltage range against fsd soc
    const CELL100: f32 = 4175.0;
    const CELL0: f32 = 3500.0;

    let old_range = CELL100 - CELL0;
    let new_range = 100.0 - 0.1;
    let value: f32 = value.into();
    (((value - CELL0) * new_range) / old_range) + 0.1
}
