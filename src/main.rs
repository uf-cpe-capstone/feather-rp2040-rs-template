#![no_std]
#![no_main]

/**** low-level imports *****/
use core::fmt::Write;
use core::panic::PanicInfo;
use cortex_m::interrupt::Mutex;
use cortex_m_rt::entry;
use embedded_hal::digital::v2::OutputPin;

/***** board-specific imports *****/
use adafruit_feather_rp2040::hal;
use adafruit_feather_rp2040::{Pins, XOSC_CRYSTAL_FREQ};
use hal::{
    clocks::{init_clocks_and_plls, Clock},
    pac,
    pac::interrupt,
    watchdog::Watchdog,
    Sio,
};

// USB Device support
use usb_device::class_prelude::*;

// USB Communications Class Device support
mod usb_manager;
use usb_manager::UsbManager;

use core::cell::RefCell;

/* Mutex is enough here, but we must add RefCell<T> since cortex_m::interrupt::Mutex
  can only give us &T, not &mut. std::sync::Mutex, however, can give us &mut T without
  needing RefCell<T>!

   Mutex<RefCell<Option<T>>>
   │     │       │
   │     │       └─ Late initialization
   │     └───────── Interior mutability (allows &T → &mut T at runtime)
   └─────────────── Interrupt safety (critical section required)
*/
// Global USB bus allocator to provide a 'static reference for USB classes
static mut USB_BUS: Option<UsbBusAllocator<hal::usb::UsbBus>> = None;

// Store the USB manager; the allocator is held in USB_BUS
static USB: Mutex<RefCell<Option<UsbManager>>> = Mutex::new(RefCell::new(None));
#[allow(non_snake_case)]
#[interrupt]
unsafe fn USBCTRL_IRQ() {
    cortex_m::interrupt::free(|cs| {
        if let Some(manager) = USB.borrow(cs).borrow_mut().as_mut() {
            manager.interrupt();
        }
    });
}

#[panic_handler]
fn panic(panic_info: &PanicInfo) -> ! {
    cortex_m::interrupt::free(|cs| {
        if let Some(usb) = USB.borrow(cs).borrow_mut().as_mut() {
            writeln!(usb, "{}", panic_info).ok();
        }
    });
    loop {}
}

#[entry]
fn main() -> ! {
    // Grab the singleton objects for
    let mut pac = pac::Peripherals::take().unwrap();
    let core = pac::CorePeripherals::take().unwrap();

    // Init the watchdog timer, to pass into the clock init
    // Can be used to recover from infinite loops (see docs)
    let mut watchdog = Watchdog::new(pac.WATCHDOG); //
    let clocks = init_clocks_and_plls(
        XOSC_CRYSTAL_FREQ,
        pac.XOSC,
        pac.CLOCKS,
        pac.PLL_SYS,
        pac.PLL_USB,
        &mut pac.RESETS,
        &mut watchdog,
    )
    .map_err(|_| ()) // InitError does not implement Debug, so let's throw it away
    .expect("Initialization of clocks and plls failed!"); // Panic with this message, if we get on error, else gimme the value

    // Setup USB
    cortex_m::interrupt::free(|cs| {
        #[allow(static_mut_refs)]
        unsafe {
            USB_BUS = Some(UsbBusAllocator::new(hal::usb::UsbBus::new(
                pac.USBCTRL_REGS,
                pac.USBCTRL_DPRAM,
                clocks.usb_clock,
                true,
                &mut pac.RESETS,
            )));

            let usb_bus_ref: &'static UsbBusAllocator<hal::usb::UsbBus> = USB_BUS.as_ref().unwrap();
            let usb_manager = UsbManager::new(usb_bus_ref);
            *USB.borrow(cs).borrow_mut() = Some(usb_manager);

            // Enable interrupt
            pac::NVIC::unmask(hal::pac::Interrupt::USBCTRL_IRQ);
        }
    });

    // Initialize the Single Cycle IO
    let sio = Sio::new(pac.SIO);

    // Initialize the pins to default state
    let pins = Pins::new(
        pac.IO_BANK0,
        pac.PADS_BANK0,
        sio.gpio_bank0,
        &mut pac.RESETS,
    );

    /* Main Program */
    /* (Init) */
    let mut timer = cortex_m::delay::Delay::new(core.SYST, clocks.system_clock.freq().to_Hz());
    let mut led_pin = pins.d13.into_push_pull_output();

    /* (Loop) */
    let delay: u32 = 500; // loop delay in ms
    let mut n: u32 = 0;
    loop {
        cortex_m::interrupt::free(|cs| {
            if let Some(usb) = USB.borrow(cs).borrow_mut().as_mut() {
                writeln!(usb, "starting loop number {}", n).unwrap();
            }
        });

        led_pin.set_low().unwrap();
        timer.delay_ms(delay as u32);

        led_pin.set_high().unwrap();
        timer.delay_ms(delay as u32);

        n = n + 1;
    }
}
