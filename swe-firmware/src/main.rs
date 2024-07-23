#![no_std]
#![no_main]

mod ble;
mod solutils;
mod wifi;

use core::mem::MaybeUninit;

use embassy_executor::Spawner;
use esp_backtrace as _;
use esp_hal::{
    clock::ClockControl,
    peripherals::Peripherals,
    prelude::*,
    rng::Rng,
    system::SystemControl,
    timer::{ErasedTimer, OneShotTimer, PeriodicTimer},
};
use esp_wifi::{initialize, EspWifiInitFor};

// When you are okay with using a nightly compiler it's better to use https://docs.rs/static_cell/2.1.0/static_cell/macro.make_static.html
#[macro_export]
macro_rules! mk_static {
    ($t:ty,$val:expr) => {{
        static STATIC_CELL: static_cell::StaticCell<$t> = static_cell::StaticCell::new();
        #[deny(unused_attributes)]
        let x = STATIC_CELL.uninit().write(($val));
        x
    }};
}

#[global_allocator]
static ALLOCATOR: esp_alloc::EspHeap = esp_alloc::EspHeap::empty();

fn init_heap() {
    const HEAP_SIZE: usize = 32 * 1024;
    static mut HEAP: MaybeUninit<[u8; HEAP_SIZE]> = MaybeUninit::uninit();

    unsafe {
        ALLOCATOR.init(HEAP.as_mut_ptr() as *mut u8, HEAP_SIZE);
    }
}

#[main]
async fn main(spawner: Spawner) {
    esp_println::logger::init_logger_from_env();
    init_heap();

    let peripherals = Peripherals::take();
    let peripherals_ptr = core::ptr::addr_of!(peripherals);

    let system = SystemControl::new(peripherals.SYSTEM);
    let clocks = ClockControl::max(system.clock_control).freeze();

    let timer: PeriodicTimer<ErasedTimer> = PeriodicTimer::new(
        esp_hal::timer::timg::TimerGroup::new(peripherals.TIMG0, &clocks, None)
            .timer0
            .into(),
    );

    {
        let systimer = esp_hal::timer::systimer::SystemTimer::new(peripherals.SYSTIMER);
        esp_hal_embassy::init(
            &clocks,
            mk_static!(
                [OneShotTimer<ErasedTimer>; 1],
                [OneShotTimer::new(systimer.alarm0.into())]
            ),
        );
    }

    #[cfg(feature = "wifi")]
    let init = initialize(
        EspWifiInitFor::Wifi,
        timer,
        Rng::new(peripherals.RNG),
        peripherals.RADIO_CLK,
        &clocks,
    )
    .unwrap();

    #[cfg(feature = "ble")]
    let init = initialize(
        EspWifiInitFor::Ble,
        timer,
        Rng::new(peripherals.RNG),
        peripherals.RADIO_CLK,
        &clocks,
    )
    .unwrap();

    #[cfg(feature = "wifi")]
    spawner
        .spawn(wifi::wifi_task(
            spawner,
            unsafe { core::ptr::read(peripherals_ptr) },
            init,
        ))
        .unwrap();

    #[cfg(feature = "ble")]
    spawner
        .spawn(ble::bt_task(
            spawner,
            unsafe { core::ptr::read(peripherals_ptr) },
            init,
        ))
        .unwrap();
}
