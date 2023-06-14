//! Blinks an LED
//!
//! This assumes that a LED is connected to pc13 as is the case on the blue pill board.
//!
//! Note: Without additional hardware, PC13 should not be used to drive an LED, see page 5.1.2 of
//! the reference manual for an explanation. This is not an issue on the blue pill.

#![no_std]
#![no_main]

use core::cell::RefCell;
use core::fmt::Write;
use cortex_m_rt::entry;
use critical_section::Mutex;
use heapless::String;
use liquidcrystal_i2c_rs::{Backlight, Display, Lcd};
use nb::block;
use panic_halt as _;
use stm32f1xx_hal::gpio::{Output, Pin};
use stm32f1xx_hal::i2c::{BlockingI2c, Mode};
use stm32f1xx_hal::{
    pac::{self, interrupt, USART1},
    prelude::*,
    serial::{Config, Rx, Serial, Tx},
    timer::Timer,
};

struct Resources {
    rx: Rx<USART1>,
    queue: heapless::Deque<u8, 50>,
}
static RESOURCES: Mutex<RefCell<Option<Resources>>> = Mutex::new(RefCell::new(None));

fn run_usart1_interrupt() {
    static ENDED: bool = false;
    critical_section::with(|t| {
        if let Some(ref mut res) = RESOURCES.borrow_ref_mut(t).as_mut() {
            let rx = &mut res.rx;
            if rx.is_rx_not_empty() {
                if let Ok(w) = nb::block!(rx.read()) {
                    res.queue.push_back(w);
                }
            }
        }
    });
}
#[interrupt]
unsafe fn USART1() {
    run_usart1_interrupt()
}

#[entry]
fn main() -> ! {
    rtt_log::init();

    // Get access to the core peripherals from the cortex-m crate
    let cp = cortex_m::Peripherals::take().unwrap();
    // Get access to the device specific peripherals from the peripheral access crate
    let dp = pac::Peripherals::take().unwrap();

    // Take ownership over the raw flash and rcc devices and convert them into the corresponding
    // HAL structs
    let mut flash = dp.FLASH.constrain();
    let rcc = dp.RCC.constrain();
    let mut dwt = cp.DWT;
    let mut syst = cp.SYST;

    // Prepare the alternate function I/O registers
    let mut afio = dp.AFIO.constrain();

    dwt.disable_cycle_counter();

    // Freeze the configuration of all the clocks in the system and store the frozen frequencies in
    // `clocks`
    let clocks = rcc.cfgr.freeze(&mut flash.acr);

    // Acquire the GPIOC peripheral
    let mut gpioc = dp.GPIOC.split();
    let mut gpioa = dp.GPIOA.split();
    let mut gpiob = dp.GPIOB.split();

    // Configure gpio C pin 13 as a push-pull output. The `crh` register is passed to the function
    // in order to configure the port. For pins 0-7, crl should be passed instead.
    let mut led = gpioc.pc13.into_push_pull_output(&mut gpioc.crh);

    let mut delay = syst.delay(&clocks);
    let mut i2c = BlockingI2c::i2c1(
        dp.I2C1,
        (
            gpiob.pb8.into_alternate_open_drain(&mut gpiob.crh),
            gpiob.pb9.into_alternate_open_drain(&mut gpiob.crh),
        ),
        &mut afio.mapr,
        Mode::Standard {
            frequency: 400.kHz(),
        },
        clocks,
        1000,
        10,
        1000,
        1000,
    );

    // USART1
    let (mut tx, mut rx) = Serial::new(
        dp.USART1,
        (
            gpioa.pa9.into_alternate_push_pull(&mut gpioa.crh),
            gpioa.pa10,
        ),
        &mut afio.mapr,
        Config::default().baudrate(115200.bps()),
        &clocks,
    )
    .split();

    write!(tx, "\x1b[2J\x1b[H").ok();
    write!(tx, "Họ và tên sinh viên: ").ok();
    rx.listen();
    critical_section::with(|t| {
        RESOURCES.replace(
            t,
            Some(Resources {
                rx,
                queue: heapless::Deque::new(),
            }),
        );
    });

    let mut lcd = Lcd::new(&mut i2c, 0x27, &mut delay).unwrap();

    lcd.set_display(Display::On).unwrap();
    lcd.set_backlight(Backlight::On).unwrap();
    lcd.print("Hello world!").unwrap();

    // Configure the syst timer to trigger an update every second
    let mut timer = Timer::new(dp.TIM1, &clocks).counter_hz();
    timer.start(1.Hz()).unwrap();

    unsafe {
        cortex_m::peripheral::NVIC::unmask(pac::Interrupt::USART1);
    }

    let mut count = 0;
    log::info!("hell no");
    loop {
        let mut s: String<20> = String::new();
        count += 1; //block!(timer.wait()).unwrap();
        if let Some(c) = critical_section::with(|t| {
            RESOURCES
                .borrow_ref_mut(t)
                .as_mut()
                .map(|res| res.queue.pop_front())
                .flatten()
        }) {
            log::info!("{c}");
            if c == 13 {
                write!(tx, "\x1b[2J\x1b[H").ok();
                write!(tx, "Họ và tên sinh viên: ").ok();
                lcd.set_cursor_position(0, 1);
            } else {
                let c = c as char;
                tx.write_char(c);
                write!(s, "{c}");
                lcd.print(&s);
            }
        }
        //lcd.home();
        //lcd.print(&s);
    }
}
