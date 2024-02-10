#![no_std]
#![no_main]
#![feature(error_in_core)]
#![feature(type_alias_impl_trait)]
#![feature(async_closure)]
#![feature(iter_array_chunks)]
#![feature(generic_arg_infer)]
extern crate alloc;
use crate::types::EthDevice;
use defmt::{debug, info, unwrap};
use embassy_executor::*;
use embassy_net::{Stack, StackResources};
use embassy_stm32::{
    gpio::{Level, Output, Speed},
    peripherals::*,
};
use embedded_alloc::Heap;
use hal::*;
use static_cell::StaticCell;

#[cfg(any(
    feature = "solax",
    feature = "foxess",
    feature = "byd",
    feature = "pylontech",
    feature = "goodwe",
    feature = "forceh2"
))]
use crate::tasks::{bms_rx, bms_tx_periodic, inverter_rx};

#[cfg(feature = "ntp")]
use embassy_stm32::rtc::{Rtc, RtcConfig};
#[cfg(feature = "syslog")]
use syslog_emb::{SyslogMessage, SyslogSocket};

pub mod config;
mod errors;
mod hal;
mod statics;
mod tasks;
mod types;
mod utils;
mod web;

#[cfg(any(feature = "ze40", feature = "ze50", feature = "tesla_m3"))]
mod wdt;

#[global_allocator]
static HEAP: Heap = Heap::empty();
use {defmt_rtt as _, panic_probe as _};

pub const MAC_ADDR: [u8; 6] = [0x00, 0x01, 0xDE, 0xAD, 0xBE, 0xEF]; // prod_device
                                                                    // const MAC_ADDR: [u8; 6] = [0x00, 0x01, 0xDE, 0xAD, 0xBE, 0xEF];  // test_device

#[embassy_executor::main]
async fn main(spawner: Spawner) -> () {
    check_compile_features();
    let p = embassy_stm32::init(peripherals_config());
    info!("Init!");

    {
        use core::mem::MaybeUninit;
        const HEAP_SIZE: usize = 1024 * 8;
        static mut HEAP_MEM: [MaybeUninit<u8>; HEAP_SIZE] = [MaybeUninit::uninit(); HEAP_SIZE];
        unsafe { HEAP.init(HEAP_MEM.as_ptr() as usize, HEAP_SIZE) }
    }

    let leds = {
        let led1 = Output::new(p.PE13.degrade(), Level::High, Speed::Medium);
        let led2 = Output::new(p.PE14.degrade(), Level::High, Speed::Medium);
        let led3 = Output::new(p.PE15.degrade(), Level::High, Speed::Medium);
        tasks::leds::Leds::new(led1, led2, led3)
    };
    defmt::unwrap!(spawner.spawn(crate::tasks::leds::led_task(leds)));
    info!("Leds task initialized");

    #[cfg(all(feature = "spi", feature = "display"))]
    {
        use embassy_stm32::spi;
        let mut spi_config = spi::Config::default();
        spi_config.frequency = embassy_stm32::time::Hertz(36_000_000);
        defmt::unwrap!(spawner.spawn(crate::tasks::display::display_task(
            spi2(p.SPI2, p.PB10, p.PC3, p.PC2, p.DMA1_CH4, p.DMA1_CH3),
            p.PE8,
            p.PE9,
            p.PE7
        )));
    }
    // UARTS
    #[cfg(any(feature = "modbus_bridge", feature = "modbus_client"))]
    let rs485 = rs485(p.USART2, p.PD6, p.PD5, p.DMA1_CH6, p.DMA1_CH5);

    let can1 = can1(p.CAN1, p.PD0, p.PD1);
    let can2 = can2(p.CAN2, p.PB5, p.PB6);
    info!("CAN peripherals initialized");

    let (device, netconfig, seed) = get_eth(
        p.ETH, p.PA1, p.PA2, p.PC1, p.PA7, p.PC4, p.PC5, p.PB12, p.PB13, p.PB11,
    )
    .await;
    // Init network stack
    info!("Ethernet initialized");
    const STACK_RESOURCES_COUNT: usize = 6;
    static STACK: StaticCell<Stack<EthDevice>> = StaticCell::new();
    static RESOURCES: StaticCell<StackResources<STACK_RESOURCES_COUNT>> = StaticCell::new();
    let stack = &*STACK.init(Stack::new(
        device,
        netconfig,
        RESOURCES.init(StackResources::<STACK_RESOURCES_COUNT>::new()),
        seed,
    ));
    info!("Ethernet stack initialized");

    /*

    App set up and tasks

    */

    #[cfg(feature = "v65")]
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
        // summer mode -> dod max 90
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

    #[cfg(any(feature = "ze40", feature = "ze50", feature = "tesla_m3"))]
    defmt::unwrap!(spawner.spawn(bms_rx()));
    #[cfg(any(
        feature = "solax",
        feature = "foxess",
        feature = "byd",
        feature = "goodwe",
        feature = "pylontech",
        feature = "forceh2"
    ))]
    {
        defmt::unwrap!(spawner.spawn(inverter_rx()));
        defmt::unwrap!(spawner.spawn(bms_tx_periodic()));
    }
    // always start can 1 first

    defmt::unwrap!(spawner.spawn(crate::tasks::can_interfaces::bms_task(can1, 500_000)));
    defmt::unwrap!(spawner.spawn(crate::tasks::can_interfaces::inverter_task(can2, 500_000)));

    // Launch network task
    unwrap!(spawner.spawn(hal::net_task(stack)));
    info!("Network task initialized");

    stack.wait_config_up().await;
    info!("Network is up");

    #[cfg(all(not(feature = "tcp_debug"), feature = "http"))]
    unwrap!(spawner.spawn(web::http::http_net_task(stack)));

    #[cfg(feature = "ntp")]
    let rtc = Rtc::new(p.RTC, RtcConfig::default());
    #[cfg(feature = "ntp")]
    unwrap!(spawner.spawn(tasks::ntp::ntp_task(stack, rtc)));

    #[cfg(all(feature = "tcp_debug", not(feature = "http")))]
    unwrap!(spawner.spawn(tasks::tcp_debug::debug_task(stack)));
    #[cfg(feature = "mqtt")]
    unwrap!(spawner.spawn(tasks::mqtt::mqtt_net_task(stack)));

    #[cfg(any(feature = "modbus_bridge", feature = "modbus_client"))]
    unwrap!(spawner.spawn(tasks::modbus::modbus_task(stack, rs485, p.PD7)));

    loop {
        debug!("Heap free: {} used: {}", HEAP.free(), HEAP.used());
        embassy_time::Timer::after(embassy_time::Duration::from_secs(10)).await;
    }
}

fn check_compile_features() {
    #[cfg(any(
        all(feature = "ze40", feature = "ze50"),
        all(feature = "ze40", feature = "tesla_m3"),
        all(feature = "ze50", feature = "tesla_m3"),
        all(feature = "solax", feature = "foxess"),
        all(feature = "solax", feature = "byd"),
        all(feature = "solax", feature = "pylontech"),
        all(feature = "foxess", feature = "byd"),
        all(feature = "foxess", feature = "pylontech"),
        all(feature = "byd", feature = "pylontech")
    ))]
    compile_error!("Only one feature in each group can be enabled at a time.");
}
