use crate::statics::*;
use defmt::{error, info, warn, Debug2Format};
use embassy_sync::blocking_mutex::raw::ThreadModeRawMutex as _Mutex;
use embassy_sync::mutex::Mutex;
use embassy_time::Instant;
use lazy_static::lazy_static;

pub type MutexData = Mutex<_Mutex, ze50_bms::Data>;

const START_TIME: i64 = 1524454107; // 23 April 2018 as epoch

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
            let pack_volts = data.pack_volts;
            #[cfg(feature = "v65")]
            let pack_volts = data.pack_volts / 6.0;

            // Calculate soc from cell millivolts
            #[cfg(feature = "v65")]
            let _soc = map_cellv_soc((data.v_high_cell + data.v_low_cell) / 2);

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
                .set_pack_volts(pack_volts)?
                .set_current(data.current_value)?
                .set_temps(data.temp_min, data.temp_max)?
                .set_kwh(data.kwh_remaining)?
                .set_max_charge_amps(max_charge_amps)?
                .set_pack_temp(data.pack_temp)?
                // .throttle_pack()? //Adjusts current ch/dis based on conditions
                .set_valid(true)?;
            defmt::debug!("Cell delta: {}mV", data.v_high_cell - data.v_low_cell);
            defmt::debug!(
                "Data: Charge max: {}A Discharge max: {}A Shunts: {} SoC {}",
                bmsdata.charge_max,
                bmsdata.discharge_max,
                "todo!", // bmsdata.get_balancing_cells(),
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
    use crate::statics::UTC_NOW;
    use embassy_futures::select::{select3 as select, Either3 as Either};
    use embassy_stm32::can::bxcan::Frame;
    use embassy_time::{Duration, Ticker, Timer};
    use embedded_hal::can::{ExtendedId, Frame as _, Id, StandardId};

    const F5D: [u8; 8] = [0xC1, 0x80, 0x5D, 0x5D, 0x00, 0x00, 0xFF, 0xCB];
    const FB2: [u8; 8] = [0xC1, 0x80, 0xB2, 0xB2, 0x00, 0x00, 0xFF, 0xCB];

    let tx = BMS_CHANNEL_TX.sender();
    let ticker_ms = |ms| Ticker::every(Duration::from_millis(ms));
    let sender = |frame| {
        if let Err(_e) = tx.try_send(frame) {
            error!("Periodic queue buf error")
        };
    };

    warn!("Starting BMS TX periodic");

    // send init
    /*
    NVROL Reset: "cansend can1 18DADBF1#021003AAAAAAAAAA && sleep 0.1 && cansend can1 18DADBF1#043101B00900AAAA"
    */
    if false {
        const PROGFRAME: [u8; 8] = [0x02, 0x10, 0x03, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA];
        let prog_frame = Frame::new(
            Id::Extended(ExtendedId::new(0x18DADBF1).unwrap()),
            &PROGFRAME,
        )
        .unwrap();
        sender(prog_frame);
        Timer::after_micros(1).await;
        let frame = Frame::new(
            Id::Extended(ExtendedId::new(0x18DADBF1).unwrap()),
            &[0x04, 0x31, 0x01, 0xB0, 0x09, 0x00, 0xAA, 0xAA],
        )
        .unwrap();
        sender(frame);
        Timer::after_micros(500).await;

        /*
        Enable temporisation before sleep: "cansend can1 18DADBF1#021003AAAAAAAAAA && sleep 0.1 && cansend can1 18DADBF1#042E928101AAAAAA"
        */

        let prog_frame = Frame::new(
            Id::Extended(ExtendedId::new(0x18DADBF1).unwrap()),
            &PROGFRAME,
        )
        .unwrap();
        sender(prog_frame);
        Timer::after_micros(1).await;
        let frame = Frame::new(
            Id::Extended(ExtendedId::new(0x18DADBF1).unwrap()),
            &[0x04, 0x2E, 0x92, 0x81, 0x01, 0xAA, 0xAA, 0xAA],
        )
        .unwrap();
        sender(frame);
        Timer::after_micros(500).await;
    }

    let mut counter = 0u8;
    let mut t200ms = ticker_ms(210);
    let mut t100ms = ticker_ms(100);

    // hold and wait for RTC to be live. Accurate time standard is essential.
    info!("Waiting for RTC");
    let mut time_payload = convert_rtc_to_payload(UTC_NOW.wait().await);
    info!("Started ZE50 Tx loop");
    loop {
        match select(t100ms.next(), t200ms.next(), UTC_NOW.wait()).await {
            Either::First(_) => {
                // iteration 0..3 5d, 4..8 b2
                counter = (counter + 1) % 9;
                let payload = if counter >= 4 { &F5D } else { &FB2 };

                // move this entirely to ZE50 crate
                let frame =
                    Frame::new(Id::Standard(StandardId::new(0x373).unwrap()), payload).unwrap();
                sender(frame);

                // move this ID to ZE50 crate
                let timeframe =
                    Frame::new(Id::Standard(StandardId::new(0x376).unwrap()), &time_payload)
                        .unwrap();
                sender(timeframe);
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
                let frame = Frame::new(
                    Id::Extended(ExtendedId::new(0x18DADBF1).unwrap()),
                    &[0x03, 0x22, pid_id, pid, 0xff, 0xff, 0xff, 0xff],
                )
                .unwrap();
                sender(frame);
            }
            Either::Third(t) => {
                time_payload = convert_rtc_to_payload(t);
            } // update time on tick (1Hz)
        };
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
                *LAST_BMS_MESSAGE.lock().await = Some(Instant::now()); // testing
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
fn push_all_to_mqtt(bms: bms_standard::Bms) {
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

/// Calc 24 bit timer - minute resolution
#[inline]
fn convert_rtc_to_payload(t: embassy_stm32::rtc::DateTime) -> [u8; 8] {
    // move this fn to ZE50 crate

    let time = chrono::NaiveDateTime::from(t);
    let duration_since_production = time.timestamp() - START_TIME;
    let minutes = (duration_since_production / 60) as u32; // 24 bits

    let year_seg = (minutes >> 16) as u8;
    let hour_seg = ((minutes) >> 8) as u8;
    let minutes_seg = minutes as u8;

    [
        year_seg,
        hour_seg,
        minutes_seg,
        year_seg,
        hour_seg,
        minutes_seg,
        0x4A,
        0x54,
    ]
}
