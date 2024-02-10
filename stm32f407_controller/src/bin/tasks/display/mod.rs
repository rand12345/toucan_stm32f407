use embassy_embedded_hal::shared_bus::asynch::spi::SpiDeviceWithConfig;
use embassy_stm32::gpio::Pin;
use embassy_stm32::{
    gpio::{Level, Output, Speed},
    peripherals::{PE7, PE8, PE9},
    spi,
};
use embassy_time::Delay;
use embedded_graphics::{
    mono_font::{ascii::FONT_8X13, MonoTextStyleBuilder},
    pixelcolor::Rgb565,
    prelude::*,
    text::*,
};
use static_cell::StaticCell;

use crate::statics::{BMS, MQTTCONFIG};
use crate::types::{Spi2Display, Spi2Interface};

#[embassy_executor::task]
pub async fn display_task(spi: Spi2Interface<'static>, dc: PE8, d_cs: PE9, rst: PE7) {
    let dc = Output::new(dc.degrade(), Level::High, Speed::VeryHigh);
    let display_cs = Output::new(d_cs.degrade(), Level::High, Speed::VeryHigh);
    let rst = Output::new(rst.degrade(), Level::High, Speed::VeryHigh);

    let mut display_config = spi::Config::default();
    display_config.frequency = embassy_stm32::time::Hertz(36_000_000);
    static SPI_BUS: StaticCell<Spi2Display> = StaticCell::new();
    let spi_bus = embassy_sync::mutex::Mutex::new(spi);
    let spi_bus = SPI_BUS.init(spi_bus);
    let spi_display = SpiDeviceWithConfig::new(spi_bus, display_cs, display_config);
    let mut display =
        st7735_embassy::ST7735::new(spi_display, dc, rst, Default::default(), 160, 131);
    display.init(&mut Delay).await.unwrap();
    display.clear(Rgb565::BLACK).unwrap();

    let bounding_box = display.bounding_box();
    let character_style = MonoTextStyleBuilder::new()
        .font(&FONT_8X13)
        .text_color(Rgb565::RED)
        .background_color(Rgb565::BLACK)
        .build();

    let left_aligned = TextStyleBuilder::new()
        .alignment(Alignment::Left)
        .baseline(Baseline::Top)
        .build();

    let center_aligned = TextStyleBuilder::new()
        .alignment(Alignment::Center)
        .baseline(Baseline::Middle)
        .build();

    let right_aligned = TextStyleBuilder::new()
        .alignment(Alignment::Right)
        .baseline(Baseline::Bottom)
        .build();

    // Text::with_text_style(
    //     "Left aligned text, origin top left",
    //     bounding_box.top_left,
    //     character_style,
    //     left_aligned,
    // )
    // .draw(&mut display)
    // .unwrap();

    // Text::with_text_style(
    //     "Center aligned text, origin center center",
    //     bounding_box.center(),
    //     character_style,
    //     center_aligned,
    // )
    // .draw(&mut display)
    // .unwrap();

    // Text::with_text_style(
    //     "Right aligned text, origin bottom right",
    //     bounding_box.bottom_right().unwrap(),
    //     character_style,
    //     right_aligned,
    // )
    // .draw(&mut display)
    // .unwrap();

    // display.flush().await.unwrap();
    let mut counter = 0u32;
    let mut s = heapless::String::<128>::new();
    use core::fmt::Write;

    loop {
        let mut display_line = |s: &str, l: i32| {
            Text::with_text_style(
                s,
                bounding_box.top_left + Point::new(0, l * 14),
                character_style,
                left_aligned,
            )
            .draw(&mut display)
            .unwrap();
        };
        let data: DisplayFormat = (*BMS.lock().await).into();
        embassy_time::Timer::after_secs(1).await;
        counter += 1;
        s.clear();
        let _ = write!(s, "C: {}", data.amps);
        defmt::info!("{}", s);
        display_line("123456789,123456789,", 0); //8x20
        display_line("1", 1);
        display_line("2", 2);
        display_line("3", 3);
        display_line("4", 4);
        display_line("5", 5);
        display_line("6", 6);
        display_line("7", 7);
        display_line("8", 8);
        display.flush().await.unwrap();
    }
}

#[derive(Clone, Copy)]
pub struct DisplayFormat {
    soc: f32,
    volts: f32,
    cell_mv_high: u16,
    cell_mv_low: u16,
    cell_temp_high: f32,
    cell_temp_low: f32,
    // #[serde(with = "BigArray")]
    // #[serde(skip)]
    // cells_millivolts: [u16; 96],
    // #[serde(skip)]
    // #[serde(with = "BigArray")]
    // cell_balance: [bool; 96],
    amps: f32,
    kwh: f32,
    charge: f32,
    discharge: f32,
    bal: u8,
    valid: bool,
}

impl From<bms_standard::Bms> for DisplayFormat {
    fn from(bmsdata: bms_standard::Bms) -> Self {
        DisplayFormat {
            soc: bmsdata.soc,
            volts: bmsdata.pack_volts,
            cell_mv_high: *bmsdata.cell_range_mv.maximum(),
            cell_mv_low: *bmsdata.cell_range_mv.minimum(),
            cell_temp_high: *bmsdata.temps.maximum(),
            cell_temp_low: *bmsdata.temps.minimum(),
            // cells_millivolts : bmsdata.cells;
            // cell_balance  bmsdata.bal_cells;
            amps: bmsdata.current,
            kwh: bmsdata.kwh_remaining,
            charge: bmsdata.charge_max,
            discharge: bmsdata.discharge_max,
            bal: bmsdata.get_balancing_cells(),
            valid: bmsdata.valid,
        }
    }
}

impl DisplayFormat {
    pub fn default() -> Self {
        Self {
            soc: 0.0,
            volts: 0.0,
            cell_mv_high: 0,
            cell_mv_low: 0,
            cell_temp_high: 0.0,
            cell_temp_low: 0.0,
            // cells_millivolts: [0; 96],
            // cell_balance: [false; 96],
            amps: 0.0,
            kwh: 0.0,
            charge: 0.0,
            discharge: 0.0,
            bal: 0,
            valid: false,
        }
    }
}
