use crate::statics::*;
use defmt::{debug, error, info, Debug2Format};
use embassy_stm32::can::bxcan::Frame;
use embassy_sync::{blocking_mutex::raw::ThreadModeRawMutex, signal::Signal};
use embassy_time::{Duration, Instant, Ticker};
use tesla_m3_bms::ExternalContactorStateCommand as ContactorState;

type ContactorType = Signal<ThreadModeRawMutex, ContactorState>;
static CONTACTOR: ContactorType = Signal::new();
const PRECHARGE_DELAY_MS: u64 = 100;
const TX_INTERVAL: u64 = 100;

/*

Integrated contactors

 */

#[cfg(feature = "tesla_m3")]
#[embassy_executor::task]
pub async fn bms_tx_periodic() {
    let tx = BMS_CHANNEL_TX.sender();
    let ticker_ms = |ms| Ticker::every(Duration::from_millis(ms));
    let sender = |frame| {
        if let Err(_e) = tx.try_send(frame) {
            error!("Periodic tx queue buf error")
        };
    };
    // Wait until can is up
    ticker_ms(4000).next().await;

    // let mut contactor_command = ContactorState::Open;
    let mut tx_interval = ticker_ms(TX_INTERVAL);
    // let mut x = 0;
    // old  &[0x41, 0x11, 0x01, 0x00, 0x00, 0x00, 0x20, 0x96]
    let _data_ac: [[u8; 8]; 6] = [
        [0x61, 0x05, 0x05, 0x00, 0x00, 0x00, 0x00, 0x8E],
        [0x60, 0x55, 0x55, 0x15, 0x54, 0x51, 0x31, 0x18],
        [0x61, 0x05, 0x05, 0x00, 0x00, 0x00, 0x40, 0xCE],
        [0x61, 0x05, 0x05, 0x00, 0x00, 0x00, 0x80, 0x0E],
        [0x60, 0x55, 0x55, 0x15, 0x54, 0x51, 0xB1, 0x98],
        [0x61, 0x05, 0x05, 0x00, 0x00, 0x00, 0xC0, 0x4E],
    ];
    let _data: [[u8; 8]; 8] = [
        [0x41, 0x01, 0x05, 0x00, 0x00, 0x00, 0x00, 0x6A],
        [0x40, 0x41, 0x05, 0x0F, 0x00, 0x50, 0x31, 0x3F],
        [0x41, 0x01, 0x05, 0x00, 0x00, 0x00, 0x40, 0xAA],
        [0x40, 0x41, 0x05, 0x0F, 0x00, 0x50, 0x51, 0x5F],
        [0x40, 0x41, 0x05, 0x0F, 0x00, 0x50, 0x91, 0x9F],
        [0x41, 0x01, 0x05, 0x00, 0x00, 0x00, 0xA0, 0x0A],
        [0x41, 0x01, 0x05, 0x00, 0x00, 0x00, 0xE0, 0x4A],
        [0x40, 0x41, 0x05, 0x0F, 0x00, 0x50, 0xF1, 0xFF],
    ];
    let arrays = [
        [0x41, 0x01, 0x05, 0x00, 0x00, 0x00, 0x00, 0x6A],
        [0x41, 0x01, 0x05, 0x00, 0x00, 0x00, 0x40, 0xAA],
        [0x40, 0x41, 0x05, 0x0F, 0x00, 0x50, 0x51, 0x5F],
        [0x41, 0x01, 0x05, 0x00, 0x00, 0x00, 0x80, 0xEA],
        [0x40, 0x41, 0x05, 0x0F, 0x00, 0x50, 0x91, 0x9F],
        [0x40, 0x41, 0x05, 0x0F, 0x00, 0x50, 0xD1, 0xDF],
    ];
    // while x < 100 {
    //     sender(frame_builder(
    //         0x332,
    //         &[0x61, 0x15, 0x01, 0x55, 0x00, 0x00, 0xe0, 0x13],
    //     ));
    //     tx_interval.next().await;
    //     x += 1
    // }
    loop {
        for data in arrays {
            tx_interval.next().await;

            // sender(frame_builder(
            //     0x221,
            //     // &[0x41, 0x11, 0x01, 0x00, 0x00, 0x00, 0x20, 0x96],
            // 0x41 0x11 0x01 0x00 0x00 0x00 0x20 0x96
            //     // &[0x41, 0x01, 0x05, 0x00, 0x00, 0x00, 0x60, 0xCA],
            //     &data,
            // ));
        }

        continue;
        // if CONTACTOR.signaled() {
        //     contactor_command = CONTACTOR.wait().await
        // }

        // /*
        // outframe.id = 0x221;            // Set our transmission address ID
        // outframe.length = 8;            // Data payload 8 bytes
        // outframe.extended = 0;          // Extended addresses - 0=11-bit 1=29bi
        // outframe.rtr=1;                 //No request
        // outframe.data.bytes[0]=0x41;
        // outframe.data.bytes[1]=0x11;
        // outframe.data.bytes[2]=0x01;
        // outframe.data.bytes[3]=0x00;
        // outframe.data.bytes[4]=0x00;
        // outframe.data.bytes[5]=0x00;
        // outframe.data.bytes[6]=0x20;
        // outframe.data.bytes[7]=0x96;

        //  */

        // // WIP
        // match contactor_command {
        //     ContactorState::Close => {
        //         sender(frame_builder(
        //             0x221,
        //             &[0x40, 0x41, 0x05, 0x15, 0x00, 0x50, 0x71, 0x7f],
        //         ));
        //         // sender(frame_builder(
        //         //     0x221,
        //         //     &[0x60, 0x55, 0x55, 0x15, 0x54, 0x51, 0xd1, 0xb8],
        //         // ));
        //     }
        //     ContactorState::Precharge => {
        //         sender(frame_builder(
        //             0x221,
        //             &[0x41, 0x11, 0x01, 0x00, 0x00, 0x00, 0x20, 0x96],
        //         ));
        //         // sender(frame_builder(
        //         //     0x221,
        //         //     &[0x61, 0x15, 0x01, 0x00, 0x00, 0x00, 0x20, 0xba],
        //         // ));
        //     }

        //     ContactorState::Open => sender(frame_builder(
        //         0x332,
        //         &[0x61, 0x15, 0x01, 0x55, 0x00, 0x00, 0xe0, 0x13],
        //     )),
        //     ContactorState::Unimplemented => continue,
        // };
    }
}

#[cfg(feature = "tesla_m3")]
#[embassy_executor::task]
pub async fn bms_rx() {
    use tesla_m3_bms::HvilState;

    let rx = BMS_CHANNEL_RX.receiver();
    let mut data = tesla_m3_bms::Data::default();
    let mut contactor_command = ContactorState::Precharge;
    let mut precharge_triggered: Option<Instant> = None;
    loop {
        let frame: Frame = rx.recv().await;
        let update = match data.decode_frame(frame) {
            Ok(update) => update,
            Err(e) => {
                error!("Tesla M3 decode error: {}", e);
                continue;
            }
        };

        if update {
            debug!("Update");
        }
        // if !matches!(data.hvil_state, HvilState::StatusOk) {
        //     error!("HVIL loop error: {}", data.hvil_state);
        //     CONTACTOR.signal(ContactorState::Open);
        //     continue;
        // }
        match (data.contactor_state, data.contactor_operation) {
            (
                tesla_m3_bms::ContactorState::Economized,
                tesla_m3_bms::ContactorOperation::Unknown6,
            ) => {
                if !matches!(contactor_command, ContactorState::Close) {
                    CONTACTOR.signal(ContactorState::Close);
                    contactor_command = ContactorState::Close
                };
                // debug!("Close")
            }
            (tesla_m3_bms::ContactorState::Open, _) => {
                if !matches!(contactor_command, ContactorState::Precharge) {
                    CONTACTOR.signal(ContactorState::Precharge);
                    contactor_command = ContactorState::Precharge
                }
            }
            (_, _) => (),
        };
        {
            CONTACTOR.signal(ContactorState::Precharge);
        }

        WDT.signal(true); // temp whilst testing

        if !update {
            continue;
        } else {
            // Debug to be removed
            debug!("Tesla Data: {:?}", Debug2Format(&data));
            let mut bms = BMS.lock().await;
            if let Err(e) = data.update_bms(&mut bms) {
                error!("Bms update error: {}", e);
                // To be changed to per-error contactor state change
                // CONTACTOR.signal(ContactorState::Open);
            } else {
                info!("Bms data updated");

                *LAST_BMS_MESSAGE.lock().await = Some(Instant::now());

                WDT.signal(true); // temp whilst testing

                // Try lock - low priority
                // if let Ok(mut lock) = MQTTFMT.try_lock() {
                //     lock.update(*bms)
                // };

                // contactor_command = match contactor_command {
                //     ContactorState::Close => continue,
                //     ContactorState::Precharge => {
                //         if let Some(time) = precharge_triggered {
                //             if time.elapsed().as_millis() < PRECHARGE_DELAY_MS {
                //                 ContactorState::Precharge
                //             } else {
                //                 precharge_triggered = None;
                //                 ContactorState::Close
                //             }
                //         } else {
                //             precharge_triggered = Some(Instant::now());
                //             ContactorState::Precharge
                //         }
                //     }
                //     ContactorState::Open | ContactorState::Unimplemented => ContactorState::Open,
                // };

                // CONTACTOR.signal(contactor_command);
            };
        }
    }
}

fn frame_builder<T: embedded_hal::can::Frame + core::clone::Clone>(id: u16, framedata: &[u8]) -> T {
    use embedded_hal::can::{Id, StandardId};
    T::new(Id::Standard(StandardId::new(id).unwrap()), framedata).unwrap()
}
