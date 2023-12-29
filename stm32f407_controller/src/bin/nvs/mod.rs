use core::hash::{Hash, Hasher};

use crate::{
    config::{ConfigTrait, JsonTrait},
    statics::{CONFIG, MQTTCONFIG, NETCONFIG},
};
use core::cell::RefCell;
use defmt::{export::panic, *};
use embassy_stm32::{gpio::Output, peripherals::*, spi::Spi};
use heapless::String;
// use heapless::String;
use siphasher::sip::SipHasher;
// use tickv::{AsyncTicKV, TicKV, MAIN_KEY};
use w25q::series25::Flash;
use ResponseData::NvsData;

use crate::types::messagebus::*;

const SECTOR_SIZE: usize = 4096;
const BUFSIZE: usize = 1024;
/// Flash chip size in Mbit.
const MEGABITS: usize = 8;

// mod errorcode;
// mod flash;
// mod hasher;
// mod kv_system;
// mod tickv;
// mod tickv_mod;
// mod utils;

//Size of the flash chip in bytes.

/*

#[embassy_executor::task]
pub async fn eeprom25q64(
    spi2_bus: Spi<'static, SPI2, DMA1_CH0, DMA1_CH1>,
    _cs_w25q64: Output<'static, PE3>,
) {
    const SIZE_IN_BYTES: usize = (MEGABITS * 1024 * 1024) / 8;
    let mut flash = Flash::init(spi2_bus, _cs_w25q64).unwrap();

    let info = flash.get_device_info().unwrap();
    debug!(
        "Block size {}, block count {}",
        info.block_size, info.block_count
    );
    debug!(
        "Page size {}, page count {}",
        info.page_size, info.page_count
    );
    debug!(
        "Sector size {}, sector count {}",
        info.sector_size, info.sector_count
    );
    // fn test_simple_append() {
    let mut read_buf: [u8; SECTOR_SIZE] = [0; SECTOR_SIZE];
    let mut hash_function = siphasher::sip::SipHasher::new();
    MAIN_KEY.hash(&mut hash_function);
    let hash = hash_function.finish();

    let tickv = AsyncTicKV::<FlashCtrl, SECTOR_SIZE>::new(
        FlashCtrl::new(flash),
        &mut read_buf,
        SIZE_IN_BYTES,
    );
    if let Err(e) = tickv.initialise(hash) {
        defmt::error!("Nvs flash init failed {}", Debug2Format(&e));
        {
            let mut con = tickv.tickv.controller.inner.borrow_mut();
            con.erase_all().unwrap()
        }
        tickv.initialise(hash).unwrap();
    };
    info!("Nvs flash init OK");
    // let mut buf: [u8; SECTOR_SIZE] = [0; SECTOR_SIZE];
    // let buf = String::new();

    let mut mbs = MESSAGEBUS.subscriber().unwrap();
    let mbp = MESSAGEBUS.publisher().unwrap();
    use crate::types::messagebus::{Message::*, RequestType::*, ResponseData::*};
    info!("Nvs task started");
    crate::statics::NVS_READY.signal(true);
    let mut read_buf: [u8; 1024] = [0; 1024];
    loop {
        // let msg = mbs.next_message_pure().await;
        let msg = match mbs.next_message().await {
            embassy_sync::pubsub::WaitResult::Lagged(m) => {
                error!("Db1 {}", Debug2Format(&m));
                continue;
            }
            embassy_sync::pubsub::WaitResult::Message(m) => m,
        };
        let req = match msg {
            Request(Nvs(req)) => req,
            Store(Nvs(req), data) => {
                let sm: &'static mut [u8; 1024] = &mut [0; 1024];
                for (idx, v) in data.as_bytes().iter().enumerate() {
                    sm[idx] = *v
                } // static mut sm: String<4> = String::new();
                if let Err((m, e)) = tickv.append_key(get_hashed_key(&req.as_bytes()), sm) {
                    if matches!(e, tickv::ErrorCode::KeyAlreadyExists) {
                        warn!("Overwriting key {}", req);
                        tickv
                            .invalidate_key(get_hashed_key(&req.as_bytes()))
                            .unwrap();
                        tickv
                            .append_key(get_hashed_key(&req.as_bytes()), sm)
                            .unwrap();
                        mbp.publish(Respond(NvsData(Some(data)))).await;
                    } else {
                        error!("Problem appending NVS key {} {}", req, Debug2Format(&e));
                        mbp.publish(Respond(NvsData(None))).await;
                    };
                    continue;
                };
                mbp.publish(Respond(NvsData(Some(data)))).await;

                embassy_time::Timer::after(embassy_time::Duration::from_millis(100)).await;
                if let Ok(b) = tickv.garbage_collect() {
                    info!("GC0: {}b", b)
                } else {
                    error!("GC0")
                };
                continue;
            }
            EraseAll => {
                if let Err(_) = tickv.tickv.controller.inner.borrow_mut().erase_all() {
                    error!("Erase NVS failed");
                } else {
                    info!("NVS erased")
                };

                defmt::panic!("Rebooting");
            }
            _ => continue,
        };
        // let mut buf2: heapless::String<SECTOR_SIZE> = heapless::String::new();

        // let mut buf2 = read_buf;
        match tickv.get_key(get_hashed_key(&req.as_bytes()), &mut read_buf) {
            Ok(_) => {
                if let Some(pos) = read_buf.into_iter().rposition(|e| e == b'}') {
                    let slice = &read_buf[0..=pos];
                    info!("Repsonding with Nvs data {}", slice);
                    // let a = unsafe { core::str::from_utf8_unchecked(slice).into() };
                    if let Ok(a) = core::str::from_utf8(slice) {
                        mbp.publish(Respond(NvsData(Some(a.into())))).await;
                    }
                }

                embassy_time::Timer::after(embassy_time::Duration::from_millis(100)).await;
                if let Ok(b) = tickv.garbage_collect() {
                    info!("GC1: {}b", b)
                } else {
                    error!("GC1")
                };
            }
            Err((m, tickv::ErrorCode::KeyNotFound)) => {
                warn!("Nvs {} KeyNotFound", req);
                mbp.publish_immediate(Respond(NvsData(None)));

                embassy_time::Timer::after(embassy_time::Duration::from_millis(100)).await;
                if let Ok(b) = tickv.garbage_collect() {
                    info!("GC2: {}b", b)
                } else {
                    error!("GC2")
                };
            }
            Err(e) => {
                error!("Pull NVS data {} {}", req, Debug2Format(&e));
            }
        }
    }

    // info!("Add Key ONE");
    // tickv.append_key(get_hashed_key(b"ONE"), &value).unwrap();

    // embassy_time::Timer::after(embassy_time::Duration::from_secs(1)).await;
    // info!("Get key ONE");
    // tickv.get_key(get_hashed_key(b"ONE"), &mut buf).unwrap();

    // embassy_time::Timer::after(embassy_time::Duration::from_secs(1)).await;
    // info!("Delete Key ONE");
    // tickv.invalidate_key(get_hashed_key(b"ONE")).unwrap();

    // info!("Get non-existant key ONE");
    // let a = b"1";
    // if let Err(e) = tickv.get_key(get_hashed_key(b"ONE"), &mut buf) {
    //     error!("Get non-existant key ONE")
    // }
}

pub async fn init() {
    init_netconfig().await;
}

pub fn nvs_erase() {
    let mbp = MESSAGEBUS.publisher().unwrap();
    mbp.publish_immediate(Message::EraseAll);
}

async fn init_netconfig() {
    {
        info!("Retrieving MQTTConfig");
        let mut mqttconfig = MQTTCONFIG.lock().await;
        retrieve_nvs_data(&mut *mqttconfig).await;
    } // let mut config = CONFIG.lock().await;
      // retrieve_nvs_data(&mut *config).await;
    {
        info!("Retrieving NetConfig");
        let mut netconfig = NETCONFIG.lock().await;
        retrieve_nvs_data(&mut *netconfig).await;
    }
}

async fn retrieve_nvs_data<T: ConfigTrait + JsonTrait + core::fmt::Debug>(config: &mut T) {
    let mut mbs = MESSAGEBUS.subscriber().unwrap();
    let mbp = MESSAGEBUS.publisher().unwrap();
    // check for stored data

    debug!("Retrieving {} from NVS", config.get_name());
    mbp.publish(Message::Request(RequestType::Nvs(config.get_name())))
        .await;
    // let json = config.to_json();
    // if json.len() > BUFSIZE {
    //     defmt::panic!("json.len() > {} ({})", BUFSIZE, json.len())
    // }
    // mbp.publish(Message::Store(RequestType::Nvs(config.get_name()), json))
    //     .await;
    /*

       Need to run the debugger here!

    */

    loop {
        // embassy_time::Timer::after(embassy_time::Duration::from_millis(1000)).await;
        debug!("Awaiting {} from NVS", config.get_name());
        let test = mbs.next_message_pure().await;
        info!("s1");
        let output = match test {
            Message::Respond(NvsData(Some(json))) => json,
            Message::Respond(NvsData(None)) => {
                warn!("NvsData(None)");
                let message = config.to_json();
                mbp.publish_immediate(Message::Store(
                    RequestType::Nvs(config.get_name()),
                    message.as_str().into(),
                ));
                break;
            }
            other => {
                debug!("Found other {}", Debug2Format(&other));
                // debug!(
                //     "other: Heap free: {} used: {}",
                //     crate::HEAP.free(),
                //     crate::HEAP.used()
                // );
                // embassy_time::Timer::after(embassy_time::Duration::from_millis(100)).await;
                continue;
            }
        };
        info!("Found {}", Debug2Format(&output));
        if let None = output.to_lowercase().find(config.get_name().as_str()) {
            warn!("Wrong NvsData response for {}", config.get_name());
            continue;
        }
        if let Some(r) = output.rfind('}') {
            let s = &output.as_bytes()[0..=r];
            // debug!("slice {}", s);
            if let Err(e) = config.from_json(s) {
                error!("NVS to struct error {}", e);
                continue;
            };
            info!("Verified Nvs {}", Debug2Format(&*config));
        };

        break;
    }
}

type NvsFlash<'a> = Flash<Spi<'a, SPI2, DMA1_CH0, DMA1_CH1>, Output<'a, PE3>>;

struct FlashCtrl<'a> {
    inner: RefCell<NvsFlash<'a>>,
}

impl<'a> FlashCtrl<'a> {
    fn new(flash: NvsFlash<'a>) -> FlashCtrl<'a> {
        FlashCtrl {
            inner: RefCell::new(flash),
        }
    }
}
// Custom allocation and deallocation functions
// unsafe fn _alloc(size: usize) -> *mut u8 {
//     alloc::alloc::alloc(
//         core::alloc::Layout::from_size_align(size, core::mem::align_of::<u8>()).unwrap(),
//     )
// }

// unsafe fn _dealloc(ptr: *mut u8, size: usize) {
//     alloc::alloc::dealloc(
//         ptr,
//         core::alloc::Layout::from_size_align(size, core::mem::align_of::<u8>()).unwrap(),
//     )
// }

impl tickv::FlashController<SECTOR_SIZE> for FlashCtrl<'_> {
    fn read_region(
        &self,
        _region_number: usize,
        _offset: usize,
        buf: &mut [u8; SECTOR_SIZE],
    ) -> Result<(), tickv::ErrorCode> {
        let mut inner = self.inner.borrow_mut();

        let address = (_region_number * SECTOR_SIZE) as u32;
        // debug!(
        //     "read_region bytes from region {} address {} offset {}",
        //     _region_number, address, _offset,
        // );
        let result = inner
            .read(address, buf)
            .map_err(|_| tickv::ErrorCode::ReadFail);

        // debug!("Read : {}b", buf.len());

        result
    }

    fn write(&self, _address: usize, buf: &[u8]) -> Result<(), tickv::ErrorCode> {
        use alloc::vec::Vec;
        let mut inner = self.inner.borrow_mut();
        debug!("write bytes to address {} : {:x}", _address, buf,);
        let buf_len = buf.len();
        debug!("Write : {}b", buf_len);

        // Allocate memory for the mutable copy
        // let buf_v2 = unsafe {
        //     let buf_v2_ptr = _alloc(buf_len);
        //     core::ptr::copy_nonoverlapping(buf.as_ptr(), buf_v2_ptr, buf_len);
        //     buf_v2_ptr
        // };
        let mut buf_v3: Vec<u8> = Vec::new();
        buf_v3.extend_from_slice(buf);
        let result = inner
            .write_bytes(_address as u32, &mut buf_v3)
            // .write_bytes(_address as u32, unsafe {
            //     core::slice::from_raw_parts_mut(buf_v2, buf_len)
            // })
            .map_err(|_| tickv::ErrorCode::WriteFail);

        // unsafe {
        //     _dealloc(buf_v2, buf_len);
        // }
        result
    }

    fn erase_region(&self, _region_number: usize) -> Result<(), tickv::ErrorCode> {
        debug!("Erasing region {}", _region_number);
        self.inner
            .borrow_mut()
            .erase_block(_region_number.try_into().unwrap())
            .map_err(|_| tickv::ErrorCode::EraseFail)
    }
}

fn get_hashed_key(unhashed_key: &[u8]) -> u64 {
    let mut hash_function = SipHasher::new();
    unhashed_key.hash(&mut hash_function);
    hash_function.finish()
}
 */
