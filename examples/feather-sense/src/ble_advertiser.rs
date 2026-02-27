//! BLE advertiser using nrf-sdc (Nordic SoftDevice Controller)
//!
//! Advertises as "PEAT-xxxx" where xxxx is derived from device ID.
//! Watch for it with nRF Connect on your phone.

#![no_std]
#![no_main]

use bt_hci::cmd::le::{LeSetAdvData, LeSetAdvEnable, LeSetAdvParams};
use bt_hci::cmd::SyncCmd;
use bt_hci::param::BdAddr;
use defmt::{info, unwrap};
use embassy_executor::Spawner;
use embassy_nrf::gpio::{Level, Output, OutputDrive};
use embassy_nrf::mode::Blocking;
use embassy_nrf::rng::{self, Rng};
use embassy_nrf::{bind_interrupts, pac, peripherals::RNG};
use embassy_time::{Duration, Timer};
use nrf_sdc::{self as sdc, mpsl};
use sdc::mpsl::MultiprotocolServiceLayer;
use sdc::vendor::ZephyrWriteBdAddr;
use static_cell::StaticCell;
use {defmt_rtt as _, panic_probe as _};

bind_interrupts!(struct Irqs {
    RNG => rng::InterruptHandler<RNG>;
    EGU0_SWI0 => mpsl::LowPrioInterruptHandler;
    CLOCK_POWER => mpsl::ClockInterruptHandler;
    RADIO => mpsl::HighPrioInterruptHandler;
    TIMER0 => mpsl::HighPrioInterruptHandler;
    RTC0 => mpsl::HighPrioInterruptHandler;
});

fn build_sdc<'d, const N: usize>(
    p: nrf_sdc::Peripherals<'d>,
    rng: &'d mut Rng<'static, Blocking>,
    mpsl: &'d MultiprotocolServiceLayer,
    mem: &'d mut sdc::Mem<N>,
) -> Result<nrf_sdc::SoftdeviceController<'d>, nrf_sdc::Error> {
    sdc::Builder::new()?.support_adv()?.build(p, rng, mpsl, mem)
}

fn bd_addr() -> BdAddr {
    let ficr = pac::FICR;
    let high = u64::from(ficr.deviceid(1).read());
    let addr = high << 32 | u64::from(ficr.deviceid(0).read());
    // Set the upper bits to indicate a random static address
    let addr = addr | 0x0000_c000_0000_0000;
    BdAddr::new(unwrap!(addr.to_le_bytes()[..6].try_into()))
}

fn build_device_name() -> [u8; 9] {
    let ficr = pac::FICR;
    let id = ficr.deviceid(0).read();
    let suffix = (id & 0xFFFF) as u16;

    let hex = b"0123456789ABCDEF";
    [
        b'H', b'I', b'V', b'E', b'-',
        hex[((suffix >> 12) & 0xF) as usize],
        hex[((suffix >> 8) & 0xF) as usize],
        hex[((suffix >> 4) & 0xF) as usize],
        hex[(suffix & 0xF) as usize],
    ]
}

#[embassy_executor::task]
async fn mpsl_task(mpsl: &'static MultiprotocolServiceLayer<'static>) -> ! {
    mpsl.run().await
}

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    info!("BLE Advertiser starting...");

    // Initialize embassy with default clocks (internal oscillators)
    let p = embassy_nrf::init(Default::default());

    // LED for status (P1.09 is red LED on Feather Sense)
    let mut led = Output::new(p.P1_09, Level::Low, OutputDrive::Standard);

    // Quick blink to show we started
    led.set_high();
    Timer::after(Duration::from_millis(100)).await;
    led.set_low();

    info!("Initializing MPSL...");

    // MPSL peripherals
    let mpsl_p = mpsl::Peripherals::new(p.RTC0, p.TIMER0, p.TEMP, p.PPI_CH19, p.PPI_CH30, p.PPI_CH31);

    // Low-frequency clock config (use internal RC oscillator)
    let lfclk_cfg = mpsl::raw::mpsl_clock_lfclk_cfg_t {
        source: mpsl::raw::MPSL_CLOCK_LF_SRC_RC as u8,
        rc_ctiv: mpsl::raw::MPSL_RECOMMENDED_RC_CTIV as u8,
        rc_temp_ctiv: mpsl::raw::MPSL_RECOMMENDED_RC_TEMP_CTIV as u8,
        accuracy_ppm: mpsl::raw::MPSL_DEFAULT_CLOCK_ACCURACY_PPM as u16,
        skip_wait_lfclk_started: mpsl::raw::MPSL_DEFAULT_SKIP_WAIT_LFCLK_STARTED != 0,
    };

    static MPSL: StaticCell<MultiprotocolServiceLayer> = StaticCell::new();
    let mpsl = MPSL.init(unwrap!(mpsl::MultiprotocolServiceLayer::new(mpsl_p, Irqs, lfclk_cfg)));
    spawner.spawn(unwrap!(mpsl_task(mpsl)));

    info!("Initializing SDC...");

    // SDC peripherals (PPI channels)
    let sdc_p = sdc::Peripherals::new(
        p.PPI_CH17, p.PPI_CH18, p.PPI_CH20, p.PPI_CH21, p.PPI_CH22, p.PPI_CH23,
        p.PPI_CH24, p.PPI_CH25, p.PPI_CH26, p.PPI_CH27, p.PPI_CH28, p.PPI_CH29,
    );

    // RNG for BLE
    static RNG_CELL: StaticCell<Rng<'static, Blocking>> = StaticCell::new();
    let rng = RNG_CELL.init(rng::Rng::new_blocking(p.RNG));

    // SDC memory
    static SDC_MEM: StaticCell<sdc::Mem<4096>> = StaticCell::new();
    let sdc_mem = SDC_MEM.init(sdc::Mem::new());

    let sdc = unwrap!(build_sdc(sdc_p, rng, mpsl, sdc_mem));

    info!("Configuring BLE advertising...");

    // Set the bluetooth address
    unwrap!(ZephyrWriteBdAddr::new(bd_addr()).exec(&sdc).await);

    // Build device name
    let name = build_device_name();
    let name_str = core::str::from_utf8(&name).unwrap_or("PEAT");
    info!("Device name: {}", name_str);

    // Set advertising parameters
    unwrap!(
        LeSetAdvParams::new(
            bt_hci::param::Duration::from_millis(1280),
            bt_hci::param::Duration::from_millis(1280),
            bt_hci::param::AdvKind::AdvScanInd,
            bt_hci::param::AddrKind::PUBLIC,
            bt_hci::param::AddrKind::PUBLIC,
            BdAddr::default(),
            bt_hci::param::AdvChannelMap::ALL,
            bt_hci::param::AdvFilterPolicy::default(),
        )
        .exec(&sdc)
        .await
    );

    // Build advertising data: flags + complete local name
    // Format: [len, type, data...]
    // Flags: 0x02 0x01 0x06 (LE General Discoverable, BR/EDR not supported)
    // Name: [len] 0x09 [name bytes]
    let mut adv_data = [0u8; 31];
    adv_data[0] = 0x02;  // Length of flags
    adv_data[1] = 0x01;  // AD Type: Flags
    adv_data[2] = 0x06;  // Flags value: LE General Discoverable + BR/EDR Not Supported
    adv_data[3] = (name.len() + 1) as u8;  // Length of name field
    adv_data[4] = 0x09;  // AD Type: Complete Local Name
    adv_data[5..5 + name.len()].copy_from_slice(&name);
    let adv_len = 5 + name.len();

    unwrap!(LeSetAdvData::new(adv_len as u8, adv_data).exec(&sdc).await);

    // Enable advertising
    unwrap!(LeSetAdvEnable::new(true).exec(&sdc).await);

    info!("Advertising started! Look for '{}' in nRF Connect", name_str);

    // Blink LED to show we're advertising
    loop {
        led.set_high();
        Timer::after(Duration::from_millis(300)).await;
        led.set_low();
        Timer::after(Duration::from_millis(300)).await;
    }
}
