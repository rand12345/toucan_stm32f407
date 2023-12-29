#![no_std]
#![no_main]

use defmt_rtt as _; // global logger
use embassy_stm32 as _; // memory layout
use embedded_alloc::Heap;
use panic_probe as _;

#[global_allocator]
static HEAP: Heap = Heap::empty();
use {defmt_rtt as _, panic_probe as _};

// same panicking *behavior* as `panic-probe` but doesn't print a panic message
// this prevents the panic message being printed *twice* when `defmt::panic` is invoked
#[defmt::panic_handler]
fn panic() -> ! {
    cortex_m::asm::udf()
}

// /// Terminates the application and makes `probe-run` exit with exit-code = 0
pub fn exit() -> ! {
    loop {
        cortex_m::asm::bkpt();
    }
}

#[defmt_test::tests]
#[cfg(test)]
mod unit_tests {
    use bms_standard::*;
    use defmt::assert;
    use defmt::assert_eq;
    use embassy_stm32::can::bxcan;
    use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex as _Mutex;
    use embedded_hal::can::{ExtendedId, Frame, Id, StandardId};
    use lazy_static::lazy_static;
    use solax_protocol::{SolaxBms as Inverter, SolaxError as InverterError};
    type BmsType = embassy_sync::mutex::Mutex<_Mutex, bms_standard::Bms>;

    lazy_static! {
        pub static ref BMS: BmsType = embassy_sync::mutex::Mutex::new(Bms::new(
            *Config::default().set_discharge_limts(0.0, 35.0).unwrap()
        ));
    }

    const BALDATA8: [[u8; 8]; 3] = [
        [0x10, 0x0E, 0x61, 0x07, 0x00, 0x00, 0x00, 0x00],
        [0x21, 0xFF, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        [0x22, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
    ];
    const BALDATA96: [[u8; 8]; 3] = [
        [0x10, 0x0E, 0x61, 0x07, 0xFF, 0xFF, 0xFF, 0xFF],
        [0x21, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF],
        [0x22, 0xFF, 0xFF, 0x00, 0x00, 0x00, 0x00, 0x00],
    ];

    const CELL1DATA: [[u8; 8]; 19] = [
        [0x10, 0x7E, 0x61, 0x41, 0x0E, 0xD1, 0x0E, 0xD0],
        [0x21, 0x0E, 0xCF, 0x0E, 0xD1, 0x0E, 0xD0, 0x0E],
        [0x22, 0xD0, 0x0E, 0xD0, 0x0E, 0xD2, 0x0E, 0xCF],
        [0x23, 0x0E, 0xD0, 0x0E, 0xCA, 0x0E, 0xCD, 0x0E],
        [0x24, 0xCC, 0x0E, 0xCD, 0x0E, 0xCB, 0x0E, 0xCF],
        [0x25, 0x0E, 0xCF, 0x0E, 0xCF, 0x0E, 0xCD, 0x0E],
        [0x26, 0xCC, 0x0E, 0xCD, 0x0E, 0xCF, 0x0E, 0xCA],
        [0x27, 0x0E, 0xCC, 0x0E, 0xD0, 0x0E, 0xC9, 0x0E],
        [0x28, 0xCB, 0x0E, 0xC9, 0x0E, 0xCD, 0x0E, 0xCB],
        [0x29, 0x0E, 0xC5, 0x0E, 0xC5, 0x0E, 0xD4, 0x0E],
        [0x2A, 0xCF, 0x0E, 0xC6, 0x0E, 0xCD, 0x0E, 0xCC],
        [0x2B, 0x0E, 0xCC, 0x0E, 0xCA, 0x0E, 0xCD, 0x0E],
        [0x2C, 0xD1, 0x0E, 0xD1, 0x0E, 0xCD, 0x0E, 0xCC],
        [0x2D, 0x0E, 0xD0, 0x0E, 0xD0, 0x0E, 0xCF, 0x0E],
        [0x2E, 0xD2, 0x0E, 0xD1, 0x0E, 0xD4, 0x0E, 0xD2],
        [0x2F, 0x0E, 0xD0, 0x0E, 0xD0, 0x0E, 0xD0, 0x0E],
        [0x20, 0xD2, 0x0E, 0xD1, 0x0E, 0xD2, 0x0E, 0xD0],
        [0x21, 0x0E, 0xCF, 0x0E, 0xD0, 0x0E, 0xCF, 0x0E],
        [0x22, 0xD0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
    ];

    const CELL2DATA: [[u8; 8]; 11] = [
        [0x10, 0x4A, 0x61, 0x42, 0x0E, 0xCB, 0x0E, 0xD1],
        [0x21, 0x0E, 0xCD, 0x0E, 0xD0, 0x0E, 0xCD, 0x0E],
        [0x22, 0xCC, 0x0E, 0xC5, 0x0E, 0xC7, 0x0E, 0xC4],
        [0x23, 0x0E, 0xC5, 0x0E, 0xC9, 0x0E, 0xC2, 0x0E],
        [0x24, 0xC4, 0x0E, 0xC2, 0x0E, 0xBB, 0x0E, 0xC0],
        [0x25, 0x0E, 0xC0, 0x0E, 0xCC, 0x0E, 0xD0, 0x0E],
        [0x26, 0xC7, 0x0E, 0xD0, 0x0E, 0xC7, 0x0E, 0xCB],
        [0x27, 0x0E, 0xC5, 0x0E, 0xC7, 0x0E, 0xD1, 0x0E],
        [0x28, 0xD0, 0x0E, 0xD4, 0x0E, 0xD1, 0x0E, 0xD0],
        [0x29, 0x0E, 0xCD, 0x0E, 0xCD, 0x0E, 0xD2, 0x0E],
        [0x2A, 0xD5, 0x8E, 0x19, 0x8E, 0x46, 0x00, 0x00],
    ];

    const TEMPDATA: [[u8; 8]; 12] = [
        [0x10, 0x4D, 0x61, 0x04, 0x09, 0x74, 0x38, 0x09],
        [0x21, 0x74, 0x38, 0x09, 0x63, 0x39, 0x09, 0x57],
        [0x22, 0x39, 0x09, 0x52, 0x39, 0x09, 0x60, 0x39],
        [0x23, 0x09, 0x43, 0x39, 0x09, 0x3A, 0x3A, 0x09],
        [0x24, 0x37, 0x3A, 0x09, 0x48, 0x39, 0x09, 0x55],
        [0x25, 0x39, 0x09, 0x65, 0x39, 0x08, 0xD0, 0x3C],
        [0x26, 0x08, 0xE9, 0x3C, 0xFF, 0xFF, 0xFF, 0xFF],
        [0x27, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF],
        [0x28, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF],
        [0x29, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF],
        [0x2A, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x38, 0x39],
        [0x2B, 0x3C, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
    ];

    #[test]
    fn solax_can_test() {
        let mut solax = Inverter::default();

        let bms_mutex = &BMS;
        // let mut bms = Bms::new(Config::default());
        let inverter_frame = |id: u32, payload: &[u8; 8]| {
            bxcan::Frame::new(Id::Extended(ExtendedId::new(id).unwrap()), payload).unwrap()
        };
        let request = inverter_frame(0x1871, &[1, 0, 1, 0, 0, 0, 0, 0]);

        {
            let mut bms = bms_mutex.try_lock().unwrap();
            assert!(bms.set_soc(42.1).is_ok());
            assert!(bms.set_max_charge_amps(50.0).is_ok());
            assert!(bms.set_max_discharge_amps(35.0).is_ok());
            assert!(bms.set_current(1.0).is_ok());
            assert!(bms.set_cell_mv_low_high(4000, 4100).is_ok());
            assert!(bms.set_temps(22.0, 23.0).is_ok());
            assert!(bms.set_pack_volts(375.5).is_ok());
            assert!(bms.set_kwh(12.5).is_ok());
            assert!(bms.set_pack_temp(22.5).is_ok());
            assert!(bms.set_valid(true).is_ok());
        };

        let response = {
            let bms = bms_mutex.try_lock().unwrap();
            solax.parser(request.clone(), &bms, true)
        };
        match response {
            Ok(mut testf) => {
                // let mut testf = frames.iter();
                let f = testf.next();
                assert_eq!(0, f.clone().unwrap().dlc());
                assert!(
                    bxcan::Id::Extended(bxcan::ExtendedId::new(0x100A001).unwrap())
                        == f.unwrap().id()
                );
                let f = testf.next();
                assert_eq!(None, f);
            }
            Err(e) => panic!("{:?}", e),
        };

        let response = {
            let bms = bms_mutex.try_lock().unwrap();
            solax.parser(request.clone(), &bms, true)
        };
        match response {
            Ok(mut f) => {
                // let mut f = frames.iter();

                let testf = f.next().unwrap();
                assert_eq!(8, testf.dlc());
                assert!(bxcan::Id::Extended(bxcan::ExtendedId::new(0x1877).unwrap()) == testf.id());
                assert_eq!([0, 0, 0, 0, 0, 0, 247, 22][..], testf.data().unwrap()[..]);

                let testf = f.next().unwrap();
                assert!(bxcan::Id::Extended(bxcan::ExtendedId::new(0x1872).unwrap()) == testf.id());
                assert_eq!(
                    [0xa0, 0x0f, 0xb8, 0x0b, 0xf4, 0x01, 0x5e, 0x01][..],
                    testf.data().unwrap()[..]
                );

                let testf = f.next().unwrap();
                assert!(bxcan::Id::Extended(bxcan::ExtendedId::new(0x1873).unwrap()) == testf.id());
                assert_eq!(
                    [0xab, 0x0e, 0x0a, 0x00, 0x2a, 0x00, 0xe2, 0x04][..],
                    testf.data().unwrap()[..]
                );

                let testf = f.next().unwrap();
                assert!(bxcan::Id::Extended(bxcan::ExtendedId::new(0x1874).unwrap()) == testf.id());
                assert_eq!(
                    [0xe6, 0x00, 0xdc, 0x00, 0x29, 0x00, 0x28, 0x00][..],
                    testf.data().unwrap()[..]
                );

                let testf = f.next().unwrap();
                assert!(bxcan::Id::Extended(bxcan::ExtendedId::new(0x1875).unwrap()) == testf.id());
                assert_eq!(
                    [0xe1, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00][..],
                    testf.data().unwrap()[..]
                );

                let testf = f.next().unwrap();
                assert!(bxcan::Id::Extended(bxcan::ExtendedId::new(0x1876).unwrap()) == testf.id());
                assert_eq!(
                    [0x01, 0x00, 0x04, 0x10, 0x00, 0x00, 0xa0, 0x0f][..],
                    testf.data().unwrap()[..]
                );

                let testf = f.next().unwrap();
                assert!(bxcan::Id::Extended(bxcan::ExtendedId::new(0x1878).unwrap()) == testf.id());
                assert_eq!(
                    [0xab, 0x0e, 0x00, 0x00, 0x10, 0x27, 0x00, 0x00][..],
                    testf.data().unwrap()[..]
                );
                assert_eq!(None, f.next());
            }
            Err(e) => panic!("{:?}", e),
        };
    }

    use ze40_bms::{can_frames::*, BMSStatus, ChargingState, Data, RequestMode};

    #[test]
    fn ze40_preamble_test() {
        // const IDLE: u8 = 0x30;
        // const TRANSITION: u8 = 0x33;
        // const CHARGING: u8 = 0x07;
        // const DRIVING: u8 = 0x0B;
        // const TEMP: u8 = 20 * 10;
        // Preamble test A

        let transitition_frame = bxcan::Frame::new(
            embedded_hal::can::Id::Standard(embedded_hal::can::StandardId::new(0x423).unwrap()),
            &[0x33, 0x7F, 0xFF, 0xFF, 0xFF, 0xE0, 0xFF, 0xFF],
        );
        let idle_frame = bxcan::Frame::new(
            embedded_hal::can::Id::Standard(embedded_hal::can::StandardId::new(0x423).unwrap()),
            &[0x30, 0x7F, 0xFF, 0xFF, 0xFF, 0xE0, 0xFF, 0xFF],
        );

        // Charging test
        {
            let f = request_frame::<bxcan::Frame>(ChargingState::Charging, &Some(LbcKey::Xb2));
            assert!(f.is_some());
            let expected_frame = bxcan::Frame::new(
                embedded_hal::can::Id::Standard(embedded_hal::can::StandardId::new(0x423).unwrap()),
                &[0x7, 0x1D, 0x00, 0x02, 0xb2, 0x80, 0xb2, 200],
            );
            assert_eq!(f, expected_frame);

            let f = request_frame::<bxcan::Frame>(ChargingState::Charging, &Some(LbcKey::X5d));
            assert!(f.is_some());
            let expected_frame = bxcan::Frame::new(
                embedded_hal::can::Id::Standard(embedded_hal::can::StandardId::new(0x423).unwrap()),
                &[0x7, 0x1D, 0x00, 0x02, 0x5d, 0x80, 0x5d, 200],
            );
            assert_eq!(f, expected_frame);

            let f = request_frame::<bxcan::Frame>(ChargingState::Charging, &None);
            assert!(f.is_some());
            assert_eq!(f, transitition_frame);
        }

        // Transition to charging test
        {
            let f = request_frame::<bxcan::Frame>(ChargingState::Discharing, &None);
            assert!(f.is_some());
            assert_eq!(f, transitition_frame);

            // Transition to charging test
            let f = request_frame::<bxcan::Frame>(ChargingState::Discharing, &Some(LbcKey::X5d));
            assert!(f.is_some());
            assert_eq!(f, idle_frame);

            // Transition to charging test
            let f = request_frame::<bxcan::Frame>(ChargingState::Discharing, &Some(LbcKey::X5d));
            assert!(f.is_some());
            assert_eq!(f, idle_frame);
        }

        // Driving (normal mode) test
        {
            let f = request_frame::<bxcan::Frame>(ChargingState::Driving, &Some(LbcKey::Xb2));
            assert!(f.is_some());
            let expected_frame = bxcan::Frame::new(
                embedded_hal::can::Id::Standard(embedded_hal::can::StandardId::new(0x423).unwrap()),
                &[0x0B, 0x1D, 0x00, 0x02, 0xb2, 0x80, 0xb2, 200],
            );
            assert_eq!(f, expected_frame);

            let f = request_frame::<bxcan::Frame>(ChargingState::Driving, &Some(LbcKey::X5d));
            assert!(f.is_some());
            let expected_frame = bxcan::Frame::new(
                embedded_hal::can::Id::Standard(embedded_hal::can::StandardId::new(0x423).unwrap()),
                &[0x0B, 0x1D, 0x00, 0x02, 0x5D, 0x80, 0x5D, 200],
            );
            assert_eq!(f, expected_frame);
            // Transition to charging test
            let f = request_frame::<bxcan::Frame>(ChargingState::Driving, &None);
            assert!(f.is_some());
            assert_eq!(f, transitition_frame);
        }
    }

    #[test]
    fn test_kangoo_rapid_can_data() {
        let mut data = Data::new();
        let mut bms = {
            let mut bms = BMS.try_lock().unwrap();
            *bms = Bms::new(*Config::default().set_discharge_limts(0.0, 35.0).unwrap());
            bms
        };
        let r1 = data.rapid_data_processor::<bxcan::Frame>(
            Frame::new(
                Id::Standard(StandardId::new(0x155).unwrap()),
                &[0x2E, 0x47, 0xCB, 0x54, 0x67, 0x00, 0x02, 0xD9],
            )
            .unwrap(),
        );
        assert!(r1.is_ok());
        let r2 = data.rapid_data_processor::<bxcan::Frame>(
            Frame::new(
                Id::Standard(StandardId::new(0x424).unwrap()),
                &[0x11, 0x40, 0x56, 0x84, 0x39, 0x6C, 0xF8, 0x3C],
            )
            .unwrap(),
        );
        assert!(r2.is_ok());
        let r3 = data.rapid_data_processor::<bxcan::Frame>(
            Frame::new(
                Id::Standard(StandardId::new(0x425).unwrap()),
                &[0x24, 0xDC, 0x44, 0x9C, 0x42, 0x2E, 0xE1, 0x15],
            )
            .unwrap(),
        );
        assert!(r3.is_ok());

        let r4 = data.rapid_data_processor::<bxcan::Frame>(
            Frame::new(
                Id::Standard(StandardId::new(0x4ae).unwrap()),
                &[0x24, 0xDC, 0x44, 0x9C, 0x42, 0x2E, 0xE1, 0x15],
            )
            .unwrap(),
        );
        assert!(r4.is_ok());
        assert!(r4.unwrap()); // check update mode
        assert_eq!(data.soc_value, 65);
        assert_eq!(data.max_charge_amps, 46);
        assert_eq!(data.current_value, -1.25);
        assert_eq!(data.kwh_remaining, 22.0);
        assert_eq!(data.pack_volts, 0.0);
        assert_eq!(data.pack_temp, 17.0);
        assert!(data.mode == BMSStatus::BMSReady);

        // test bms update
        assert!(bms.set_valid(false).is_ok());
        assert!(bms.set_current(data.current_value).is_ok());
        assert!(bms.set_soc(data.soc_value).is_ok());
        assert!(bms.set_kwh(data.kwh_remaining).is_ok());
        assert!(bms.set_max_charge_amps(data.max_charge_amps).is_ok());
        assert!(bms.set_pack_temp(data.pack_temp).is_ok());
        assert!(bms.set_valid(true).is_ok());

        let rframe = bxcan::Frame::new(
            embedded_hal::can::Id::Standard(embedded_hal::can::StandardId::new(0x79b).unwrap()),
            &[0x30, 0x01, 0x64, 0x00, 0x00, 0x00, 0x00, 0x00],
        );
        let temp_request_frame = bxcan::Frame::new(
            embedded_hal::can::Id::Standard(embedded_hal::can::StandardId::new(0x79b).unwrap()),
            &[2, 33, 4, 0, 0, 0, 0, 0],
        );
        let bal_request_frame = bxcan::Frame::new(
            embedded_hal::can::Id::Standard(embedded_hal::can::StandardId::new(0x79b).unwrap()),
            &[2, 33, 7, 0, 0, 0, 0, 0],
        );

        let x7bb = |data: &[u8; 8]| {
            bxcan::Frame::new(
                embedded_hal::can::Id::Standard(embedded_hal::can::StandardId::new(0x7bb).unwrap()),
                data,
            )
            .unwrap()
        };

        let cellbank2_request = bxcan::Frame::new(
            embedded_hal::can::Id::Standard(embedded_hal::can::StandardId::new(0x79b).unwrap()),
            &[0x02, 0x21, 0x42, 0x00, 0x00, 0x00, 0x00, 0x00],
        );

        for payload in CELL1DATA.iter() {
            let r = data.diag_data_processor::<bxcan::Frame>(x7bb(&payload));
            if payload != CELL1DATA.last().unwrap() {
                assert_eq!(rframe, r.unwrap());
            } else {
                assert_eq!(cellbank2_request, r.unwrap());
            }

            assert_eq!(data.req_mode, RequestMode::CellBank1)
        }

        for payload in CELL2DATA.iter() {
            let r = data.diag_data_processor::<bxcan::Frame>(x7bb(&payload));
            if payload != CELL2DATA.last().unwrap() {
                assert_eq!(rframe, r.unwrap());
            } else {
                assert_eq!(bal_request_frame, r.unwrap())
            }
            assert_eq!(data.req_mode, RequestMode::CellBank2)
        }
        assert_eq!(data.pack_volts, 363.77);
        assert_eq!(*data.cell_mv.minimum(), 3771);
        assert_eq!(*data.cell_mv.maximum(), 3797);

        // [(BALDATA8, 8), (BALDATA96, 96)]
        [(BALDATA8, 4), (BALDATA96, 52)] //4 is wrong, should be 8, 52 -> 96
            .into_iter()
            .for_each(|(baldata, bal)| {
                for payload in baldata.iter() {
                    let r = data
                        .diag_data_processor::<bxcan::Frame>(x7bb(&payload))
                        .unwrap();
                    if payload != baldata.last().unwrap() {
                        assert_eq!(r, rframe)
                    } else {
                        assert_eq!(r, temp_request_frame)
                    }
                    assert_eq!(data.req_mode, RequestMode::Balance)
                }
                assert_eq!(bal, data.get_balance_cells());
            });

        for payload in TEMPDATA.iter() {
            let r = data.diag_data_processor::<bxcan::Frame>(x7bb(&payload));
            if payload != TEMPDATA.last().unwrap() {
                assert_eq!(rframe, r.unwrap());
            } else {
                assert_eq!(None, r.unwrap());
            }
            assert_eq!(data.req_mode, RequestMode::Temperature)
        }
        assert_eq!(
            [16, 16, 17, 17, 17, 17, 17, 18, 18, 17, 17, 17, 20, 20],
            data.temps
        );
        assert_eq!(16, *data.temp.minimum());
        assert_eq!(20, *data.temp.maximum());

        assert!(bms.set_valid(false).is_ok());
        assert!(!bms.valid);
        assert!(bms.set_valid(true).is_ok());
        assert!(bms.valid);
        assert!(bms.set_pack_volts(data.pack_volts).is_ok());
        assert_eq!(bms.pack_volts, 363.77);
        assert!(bms.set_cell_mv(data.cells_mv).is_ok());
        assert!(bms
            .set_temps(*data.temp.minimum(), *data.temp.maximum())
            .is_ok());
        assert!(bms.throttle_pack().is_ok());
        assert_eq!(bms.soc, 65 as f32);
        assert_eq!(bms.pack_volts, 363.77);
        assert_eq!(*bms.cell_range_mv.minimum(), 3771);
        assert_eq!(*bms.cell_range_mv.maximum(), 3797);
        assert_eq!(bms.charge_max, 46.0);
        assert_eq!(bms.current, -1.25);
        assert_eq!(bms.kwh_remaining, 22.0);
        assert_eq!(bms.temp, 17.0);
    }
}
