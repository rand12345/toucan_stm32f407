#![no_std]
#![no_main]
#![feature(error_in_core)]
#![feature(type_alias_impl_trait)]
#![feature(async_closure)]
#![feature(iter_array_chunks)]
extern crate alloc;
use defmt::*;
use embassy_executor::*;
use embassy_net::{Stack, StackResources};
use embassy_stm32::{
    gpio::{Level, Output, Speed},
    peripherals::ETH,
};
use embedded_alloc::Heap;
use hal::*;
use static_cell::make_static;

#[cfg(feature = "tesla_m3")]
use crate::tasks::can_processors_tesla_m3::*;
#[cfg(feature = "ze40")]
use crate::tasks::can_processors_ze40::*;

#[cfg(feature = "ze50")]
use crate::tasks::can_processors_ze50::*;

#[cfg(any(feature = "foxess", feature = "solax"))]
use crate::tasks::can_processors_solax::*;

#[cfg(any(feature = "pylontech", feature = "byd"))]
use crate::tasks::can_processors_pylontech::*;

#[global_allocator]
static HEAP: Heap = Heap::empty();
use {defmt_rtt as _, panic_probe as _};

pub mod config;
mod errors;
mod hal;
#[cfg(feature = "nvs")]
mod nvs;
mod statics;
mod tasks;
mod types;
mod utils;
mod wdt;
mod web;

pub const MAC_ADDR: [u8; 6] = [0x00, 0x00, 0xDE, 0xAD, 0xBE, 0xEF]; // prod_device
                                                                    // const MAC_ADDR: [u8; 6] = [0x00, 0x01, 0xDE, 0xAD, 0xBE, 0xEF];  // test_device

#[embassy_executor::main]
async fn main(spawner: Spawner) -> () {
    let p = embassy_stm32::init(peripherals_config());
    info!("Init!");

    {
        use core::mem::MaybeUninit;
        const HEAP_SIZE: usize = 1024 * 8;
        static mut HEAP_MEM: [MaybeUninit<u8>; HEAP_SIZE] = [MaybeUninit::uninit(); HEAP_SIZE];
        unsafe { HEAP.init(HEAP_MEM.as_ptr() as usize, HEAP_SIZE) }
    }

    // SPI - 8 pin header & eeprom
    #[cfg(feature = "nvs")]
    let spi2_bus = spi2(&p);

    let leds = {
        let led1 = Output::new(p.PE13.degrade(), Level::High, Speed::Medium);
        let led2 = Output::new(p.PE14.degrade(), Level::High, Speed::Medium);
        let led3 = Output::new(p.PE15.degrade(), Level::High, Speed::Medium);
        tasks::leds::Leds::new(led1, led2, led3)
    };
    defmt::unwrap!(spawner.spawn(crate::tasks::leds::led_task(leds)));
    info!("Leds task initialized");

    #[cfg(feature = "nvs")]
    {
        // eeprom
        let _cs_w25q64 = Output::new(p.PE3, Level::Low, Speed::High);
        unwrap!(spawner.spawn(nvs::eeprom25q64(spi2_bus, _cs_w25q64))); // consider shared bus
        crate::statics::NVS_READY.wait().await;
        // debug!("Heap free: {} used: {}", HEAP.free(), HEAP.used());
        let _button0 = embassy_stm32::gpio::Input::new(p.PE10, embassy_stm32::gpio::Pull::Up);
        let _button1 = embassy_stm32::gpio::Input::new(p.PE11, embassy_stm32::gpio::Pull::Up);
        let _button2 = embassy_stm32::gpio::Input::new(p.PE12, embassy_stm32::gpio::Pull::Up);
        Timer::after(Duration::from_millis(1000)).await;
        info!("Initialise Nvs data and populate config structs");

        // init all nvs data
        if _button0.is_low() || _button1.is_low() || _button2.is_low() {
            info!("NVS erase selected");
            // crate::nvs::nvs_erase();
        }

        crate::nvs::init().await;

        info!("Initialise Nvs complete");
    }

    // test st7735 SPI display

    let _cs_8pin = Output::new(p.PE7, Level::Low, Speed::High);
    let _dc = Output::new(p.PE9, Level::High, Speed::High);
    let _rst = Output::new(p.PE8, Level::High, Speed::High);

    // UARTS

    let rs485 = rs485(p.USART2, p.PD6, p.PD5, p.DMA1_CH6, p.DMA1_CH5);
    // let _rx232 = rs232(&p);
    // let _usart6 = usart6(&p);

    #[cfg(feature = "sdmmc")]
    let _sdmmc = sdmmc(&p);

    // TBD
    // let _spi3 = spi3(&p);

    let can1 = can1(p.CAN1, p.PD0, p.PD1);
    let can2 = can2(p.CAN2, p.PB5, p.PB6);

    let (eth, netconfig, seed) = get_eth(
        p.ETH, p.PA1, p.PA2, p.PC1, p.PA7, p.PC4, p.PC5, p.PB12, p.PB13, p.PB11,
    )
    .await;
    // Init network stack
    const STACK_RESOURCES_COUNT: usize = 6;
    let stack = &*make_static!(Stack::new(
        eth,
        netconfig,
        make_static!(StackResources::<STACK_RESOURCES_COUNT>::new()),
        seed
    ));

    /*

    App set up and tasks

    */

    #[cfg(feature = "v65")] // MOVE TO NVS
    {
        // modify default config for Bms
        let mut bms = crate::statics::BMS.lock().await;
        bms.config.set_discharge_limts(0.0, 250.0).unwrap();
        bms.config.set_charge_limts(0.0, 250.0).unwrap();
        bms.config.set_current_sensor_limts(-200.0, 200.0).unwrap();
        // bms.set_pack_volts(55, 65).unwrap(); // might not work here
        //                                      // summer mode
        bms.set_dod(5, 90).unwrap();

        let mut config = crate::statics::CONFIG.lock().await;
        config.import_from_bms(bms.config)
    }

    #[cfg(not(feature = "v65"))]
    {
        // modify default config for Bms
        let mut bms = crate::statics::BMS.lock().await;
        bms.config.set_discharge_limts(0.0, 35.0).unwrap();
        bms.config.set_charge_limts(0.0, 150.0).unwrap();
        // bms.config.set_charge_limts(0.0, 135.0).unwrap();
        // summer mode
        bms.set_dod(5, 100).unwrap();

        let mut config = crate::statics::CONFIG.lock().await;
        config.import_from_bms(bms.config)
    }
    info!("Launching tasks!");

    use embassy_stm32::gpio::Pin;

    #[cfg(not(feature = "precharge"))]
    {
        let main_contactor = p.PA6;
        defmt::unwrap!(spawner.spawn(crate::tasks::contactor_main_task(main_contactor, p.TIM3)));
    }
    #[cfg(feature = "precharge")]
    {
        let precharge = p.PA4;
        let main_contactor = p.PA6;
        defmt::unwrap!(spawner.spawn(crate::tasks::contactor_both_task(
            precharge,
            main_contactor,
            p.TIM3
        )));
    }
    // defmt::unwrap!(spawner.spawn(crate::tasks::mqtt::uart_task(_usart6)));

    defmt::unwrap!(spawner.spawn(bms_rx()));
    defmt::unwrap!(spawner.spawn(inverter_rx()));
    defmt::unwrap!(spawner.spawn(bms_tx_periodic()));

    // // always start can 1 first

    defmt::unwrap!(spawner.spawn(crate::tasks::can_interfaces::bms_task(can1, 500_000)));
    defmt::unwrap!(spawner.spawn(crate::tasks::can_interfaces::inverter_task(can2, 500_000)));

    // Launch network task
    unwrap!(spawner.spawn(net_task(stack)));
    info!("Network task initialized");

    stack.wait_config_up().await;
    info!("Network is up");

    #[cfg(not(feature = "tcp_debug"))]
    unwrap!(spawner.spawn(web::http::http_net_task(stack)));
    #[cfg(not(feature = "tcp_debug"))]
    info!("HTTP task spawner initialized");

    unwrap!(spawner.spawn(tasks::mqtt::mqtt_net_task(stack)));
    info!("MQTT task initialized");

    unwrap!(spawner.spawn(tasks::modbus_gateway::modbus_task(stack, rs485, p.PD7)));
    info!("Modbus gateway task initialized");

    #[cfg(feature = "tcp_debug")]
    unwrap!(spawner.spawn(tasks::tcp_debug::debug_task(stack)));
    #[cfg(feature = "tcp_debug")]
    info!("TCP Debug gateway task initialized");

    loop {
        debug!("Heap free: {} used: {}", HEAP.free(), HEAP.used());
        embassy_time::Timer::after(embassy_time::Duration::from_secs(10)).await;
    }
}

// let _button0 = Input::new(p.PE10, Pull::Up);
// let _button1 = Input::new(p.PE11, Pull::Up);
// let _button2 = Input::new(p.PE12, Pull::Up);
// let _serial1rx = Input::new(p.PA9, Pull::Up); // af7 usart1
// let _serial1tx = Output::new(p.PA10, Level::High, Speed::High);
// let _can1rx = Input::new(p.PD0, Pull::Up); //af9
// let _can1tx = Output::new(p.PD1, Level::High, Speed::High);
// let _can2rx = Input::new(p.PB5, Pull::Up); //af9
// let _can2tx = Output::new(p.PB6, Level::High, Speed::High);
// let _rx485rx = Input::new(p.PD6, Pull::Up); // af7 usart2
// let _rx485tx = Output::new(p.PD5, Level::High, Speed::High);
// let mut led0 = Output::new(p.PE13, Level::Low, Speed::Low);
// let mut _led1 = Output::new(p.PE14, Level::Low, Speed::Low);
// let mut _led2 = Output::new(p.PE15, Level::High, Speed::Low);

//pc 6 - 12 SDIO

/*

{
        // Flash
        const PAGE_SIZE: usize = 4096;
        pub const FLASH_ADDR_OFFSET: usize = 0x20000000;
        pub const FLASH_WORD_SIZE: usize = 8;
        pub const FLASH_PAGES_PER_BANK: usize = 256;
        pub const FLASH_NUM_BANKS: usize = 2;
        pub const FLASH_MAX_PAGES: usize = FLASH_NUM_BANKS * FLASH_PAGES_PER_BANK;
        pub const FLASH_NUM_BUSWORDS_PER_BANK: usize = PAGE_SIZE / 4;
        pub const FLASH_MP_MAX_CFGS: usize = 8;
        // The programming windows size in words (32bit)
        pub const FLASH_PROG_WINDOW_SIZE: usize = 16;
        pub const FLASH_PROG_WINDOW_MASK: u32 = 0xFFFFFFF0;

        pub struct LowRiscPage(pub [u8; PAGE_SIZE as usize]);

        let flash_ctrl_read_buf = static_init!([u8; PAGE_SIZE], [0; PAGE_SIZE]);
        let page_buffer = static_init!(LowRiscPage, LowRiscPage::default());

        // let mux_flash = components::flash::FlashMuxComponent::new(&peripherals.flash_ctrl)
        //     .finalize(components::flash_mux_component_static!(
        //         lowrisc::flash_ctrl::FlashCtrl
        //     ));

        // SipHash
        use siphasher::sip::SipHasher24;
        let sip_hash = static_init!(SipHasher24, SipHasher24::new());
        // kernel::deferred_call::DeferredCallClient::register(sip_hash);
        // SIPHASH = Some(sip_hash);

        // TicKV
        let tickv = tickv::TicKVComponent::new(
            sip_hash,
            &mux_flash,                                    // Flash controller
            lowrisc::flash_ctrl::FLASH_PAGES_PER_BANK - 1, // Region offset (End of Bank0/Use Bank1)
            // Region Size
            lowrisc::flash_ctrl::FLASH_PAGES_PER_BANK * lowrisc::flash_ctrl::PAGE_SIZE,
            flash_ctrl_read_buf, // Buffer used internally in TicKV
            page_buffer,         // Buffer used with the flash controller
        )
        .finalize(components::tickv_component_static!(
            lowrisc::flash_ctrl::FlashCtrl,
            capsules_extra::sip_hash::SipHasher24,
            2048
        ));
        hil::flash::HasClient::set_client(&peripherals.flash_ctrl, mux_flash);
        sip_hash.set_client(tickv);
        TICKV = Some(tickv);

        let mux_kv = components::kv_system::KVStoreMuxComponent::new(tickv).finalize(
            components::kv_store_mux_component_static!(
                capsules_extra::tickv::TicKVStore<
                    capsules_core::virtualizers::virtual_flash::FlashUser<
                        lowrisc::flash_ctrl::FlashCtrl,
                    >,
                    capsules_extra::sip_hash::SipHasher24<'static>,
                    2048,
                >,
                capsules_extra::tickv::TicKVKeyType,
            ),
        );

        let kv_store = components::kv_system::KVStoreComponent::new(mux_kv).finalize(
            components::kv_store_component_static!(
                capsules_extra::tickv::TicKVStore<
                    capsules_core::virtualizers::virtual_flash::FlashUser<
                        lowrisc::flash_ctrl::FlashCtrl,
                    >,
                    capsules_extra::sip_hash::SipHasher24<'static>,
                    2048,
                >,
                capsules_extra::tickv::TicKVKeyType,
            ),
        );

        let kv_driver = components::kv_system::KVDriverComponent::new(
            kv_store,
            board_kernel,
            capsules_extra::kv_driver::DRIVER_NUM,
        )
        .finalize(components::kv_driver_component_static!(
            capsules_extra::tickv::TicKVStore<
                capsules_core::virtualizers::virtual_flash::FlashUser<
                    lowrisc::flash_ctrl::FlashCtrl,
                >,
                capsules_extra::sip_hash::SipHasher24<'static>,
                2048,
            >,
            capsules_extra::tickv::TicKVKeyType,
        ));
    }

 */

// if 1 == 2 {
//     // small  EEPROM
//     use eeprom24x::{Eeprom24x, SlaveAddr};
//     use embassy_stm32::dma::NoDma;
//     use embassy_stm32::i2c::I2c;
//     use embassy_stm32::time::Hertz;
//     let mut _i2c = I2c::new(
//         p.I2C1,
//         p.PB8, // ??
//         p.PB9, // ??
//         IrqI2c,
//         NoDma,
//         NoDma,
//         Hertz(100_000),
//         Default::default(),
//     );

//     let address = SlaveAddr::default();
//     let mut _eeprom = Eeprom24x::new_24x02(_i2c, address);
// }

// loop {
//     embassy_time::Timer::after(embassy_time::Duration::from_secs(1)).await;
//     {
//         let messagebus = types::messagebus::MESSAGEBUS.publisher().unwrap();
//         let topic = "topic";
//         let msg = types::messagebus::MqttMessage {
//             topic: topic.into(),
//             payload: "Bar".into(),
//             qos: rust_mqtt::packet::v5::publish_packet::QualityOfService::QoS0,
//             retain: false,
//         };
//         messagebus
//             .publish(types::messagebus::Message::Mqtt(msg))
//             .await
//     }
// }
// use core::fmt::Debug;
// use embedded_hal::blocking::delay::DelayUs;
// use embedded_hal::digital::v2::{InputPin, OutputPin};
// use one_wire_bus::OneWire;
// fn find_devices<P, E>(delay: &mut impl DelayUs<u16>, one_wire_pin: P)
// where
//     P: OutputPin<Error = E> + InputPin<Error = E>,
//     E: Debug,
// {
//     let mut one_wire_bus = OneWire::new(one_wire_pin).unwrap();
//     for device in one_wire_bus.devices(false, delay) {
//         // The search could fail at any time, so check each result. The iterator automatically
//         // ends after an error.
//         if let Ok(device_address) = device {
//             // The family code can be used to identify the type of device
//             // If supported, another crate can be used to interact with that device at the given address
//             debug!(
//                 "Found device at address {:?} with family code: {:x}",
//                 Debug2Format(&device_address),
//                 Debug2Format(&device_address.family_code())
//             );
//         };
//     }
// }
// let one = embassy_stm32::gpio::Flex::new(p.PE10);
// let mut delay = Delay;
// find_devices(&mut delay, one);

/*let can1tx = statics::BMS_CHANNEL_TX.sender();
let can1rx = statics::BMS_CHANNEL_RX.receiver();
let can2tx = statics::INVERTER_CHANNEL_TX.sender();
let can2rx = statics::INVERTER_CHANNEL_RX.receiver();
use embassy_stm32::can::bxcan::{Frame, StandardId};
let test_frame1 = Frame::new_data(StandardId::ZERO, [1]);
let test_frame2 = Frame::new_data(StandardId::ZERO, [2]);
// embassy_time::Timer::after(Duration::from_millis(1000)).await;
// defmt::unwrap!(spawner.spawn(crate::wdt::init(p.IWDG, 20_000_000))); // 20 seconds WDT  WHILST TESTING
defmt::unwrap!(spawner.spawn(crate::wdt::init(p.IWDG, 1_000_000))); // 20 seconds WDT  WHILST TESTING
embassy_time::Timer::after(Duration::from_secs(1)).await;
loop {
    can1tx.send(test_frame1.clone()).await;
    // embassy_time::Timer::after(Duration::from_millis(1)).await;
    can2tx.send(test_frame2.clone()).await;
    // embassy_time::Timer::after(Duration::from_millis(1)).await;
    // assert!(can1rx.receive().await, )
    info!("1rx {}", can1rx.receive().await);
    info!("2rx {}", can2rx.receive().await);
    statics::WDT.signal(true);
}
*/
