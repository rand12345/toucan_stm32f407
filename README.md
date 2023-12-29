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
* PylonTech
* Solax
* FoxESS V1
* FoxESS V2 (New)

  
Individual protocol libraries are private, feel free to get in touch.
