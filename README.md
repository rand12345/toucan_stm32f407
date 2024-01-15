# Toucan for stm32f407 platform

Fully asyncronous EV battery to ESS storage CAN bus protocol converter.
Built on the [Rust Embassy-rs framework.](https://embassy.dev)

HVDC EV battery support:

* Renault Zoe Ph1 (22kWh -> 44kWh)
* Renault Zoe Ph2 (52kWh)
* Renault Kangoo
* Tesla Model 3 (WIP - Contactors 100% ok)

Solar Hybrid battry emulation support:

* Victron (LVDC)
* BYD
* Goodwe GW6000
* PylonTech LV
* PylonTech Force H2 HVDC
* Solax
* FoxESS V1
* FoxESS V2 (cell monitoring)

Supported inverters:

* Solax
* FoxESS
* GoodWe
* Victron (BYD)
* Deye/Sunsynk
* Solis (needs testing)

Individual protocol libraries are private, feel free to get in touch.

## Hardware

[STM32F407](https://imgur.com/2hQx25R) dev board - [AliExpress $15](https://www.aliexpress.com/item/1005001620616382.html?channel=twinner ) source   
[ST Link V2](https://uk.farnell.com/stmicroelectronics/st-link-v2/icd-programmer-for-stm8-stm32/dp/1892523), [Jlink V8](https://www.amazon.co.uk/JTLB-Debugger-Emulator-Downloader-Programmer/dp/B0C72VRKTL), or STLink V2 clone (with custom harness and [adaptor](https://www.amazon.co.uk/DollaTek-J-Link-Emulator-Adapter-Converter/dp/B07L2T4N3M))


## Flashing

Download your battery-emulation combination from the releases page and flash with probe-rs.

### Install probe-rs binary (Linux/MacOS)

```curl --proto '=https' --tlsv1.2 -LsSf https://github.com/probe-rs/probe-rs/releases/download/v0.22.0/probe-rs-installer.sh | sh```

### Flash:

```probe-rs run filename.bin --chip STM32F407VETx```
