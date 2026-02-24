//! Eche Mesh Node - Pure Rust BLE GATT Server for nRF52840
//!
//! Implements the Eche GATT service for mesh networking with WearTAK.
//! Advertises as "ECHE-xxxx" with the Eche service UUID.

#![no_std]
#![no_main]

use core::mem::ManuallyDrop;
use defmt::{info, warn, unwrap};
use embassy_executor::Spawner;
use embassy_futures::join::join;
use embassy_futures::select::select;
use embassy_nrf::gpio::{Level, Output, OutputDrive};
use embassy_nrf::mode::Async;
use embassy_nrf::peripherals::RNG;
use embassy_nrf::{bind_interrupts, pac, rng};
use embassy_time::{Duration, Timer};
use nrf_sdc::mpsl::MultiprotocolServiceLayer;
use nrf_sdc::{self as sdc, mpsl};
use static_cell::StaticCell;
use trouble_host::prelude::*;
use {defmt_rtt as _, panic_probe as _};

// Eche Service UUID 16-bit (for advertising)
const ECHE_SERVICE_UUID_16BIT: u16 = 0xF47A;

bind_interrupts!(struct Irqs {
    RNG => rng::InterruptHandler<RNG>;
    EGU0_SWI0 => mpsl::LowPrioInterruptHandler;
    CLOCK_POWER => mpsl::ClockInterruptHandler;
    RADIO => mpsl::HighPrioInterruptHandler;
    TIMER0 => mpsl::HighPrioInterruptHandler;
    RTC0 => mpsl::HighPrioInterruptHandler;
});

// GATT Server definition with Eche service
#[gatt_server]
struct EcheServer {
    eche_service: EcheService,
}

/// Eche GATT Service
#[gatt_service(uuid = "f47ac10b-58cc-4372-a567-0e02b2c3d479")]
struct EcheService {
    /// Node Info - read-only node identification
    #[characteristic(uuid = "f47a0001-58cc-4372-a567-0e02b2c3d479", read, value = [0; 9])]
    node_info: [u8; 9],

    /// Sync State - sync status with notifications
    #[characteristic(uuid = "f47a0002-58cc-4372-a567-0e02b2c3d479", read, notify, value = [0; 8])]
    sync_state: [u8; 8],

    /// Sync Data - bidirectional data transfer
    #[characteristic(uuid = "f47a0003-58cc-4372-a567-0e02b2c3d479", read, write, notify, value = [0; 32])]
    sync_data: [u8; 32],

    /// Command - write-only commands from central
    #[characteristic(uuid = "f47a0004-58cc-4372-a567-0e02b2c3d479", write, value = [0; 32])]
    command: [u8; 32],

    /// Status - read-only status with notifications
    #[characteristic(uuid = "f47a0005-58cc-4372-a567-0e02b2c3d479", read, notify, value = [0; 8])]
    status: [u8; 8],
}

/// Max connections
const CONNECTIONS_MAX: usize = 1;
/// L2CAP channels (signal + ATT)
const L2CAP_CHANNELS_MAX: usize = 2;
/// L2CAP TX queue depth
const L2CAP_TXQ: u8 = 3;
/// L2CAP RX queue depth
const L2CAP_RXQ: u8 = 3;

#[embassy_executor::task]
async fn mpsl_task(mpsl: &'static MultiprotocolServiceLayer<'static>) -> ! {
    mpsl.run().await
}

fn build_sdc<'d, const N: usize>(
    p: nrf_sdc::Peripherals<'d>,
    rng: &'d mut rng::Rng<'static, Async>,
    mpsl: &'d MultiprotocolServiceLayer,
    mem: &'d mut sdc::Mem<N>,
) -> Result<nrf_sdc::SoftdeviceController<'d>, nrf_sdc::Error> {
    sdc::Builder::new()?
        .support_adv()?
        .support_peripheral()?
        .peripheral_count(1)?
        .buffer_cfg(
            DefaultPacketPool::MTU as u16,
            DefaultPacketPool::MTU as u16,
            L2CAP_TXQ,
            L2CAP_RXQ,
        )?
        .build(p, rng, mpsl, mem)
}

fn get_node_id() -> u32 {
    let ficr = pac::FICR;
    ficr.deviceid(0).read()
}

fn build_device_name_bytes() -> [u8; 9] {
    let id = get_node_id();
    let suffix = (id & 0xFFFF) as u16;

    let hex = b"0123456789ABCDEF";
    [
        b'E', b'C', b'H', b'E', b'-',
        hex[((suffix >> 12) & 0xF) as usize],
        hex[((suffix >> 8) & 0xF) as usize],
        hex[((suffix >> 4) & 0xF) as usize],
        hex[(suffix & 0xF) as usize],
    ]
}

fn build_node_info() -> [u8; 9] {
    let node_id = get_node_id();
    [
        (node_id >> 24) as u8,
        (node_id >> 16) as u8,
        (node_id >> 8) as u8,
        node_id as u8,
        1,    // protocol version
        0,    // hierarchy level (0 = peer)
        0, 0, // capabilities
        255,  // battery (255 = unknown)
    ]
}

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    info!("Eche Node starting...");

    let p = embassy_nrf::init(Default::default());

    // LED for status
    let mut led = Output::new(p.P1_09, Level::Low, OutputDrive::Standard);
    led.set_high();
    Timer::after(Duration::from_millis(100)).await;
    led.set_low();

    info!("Initializing MPSL...");

    let mpsl_p = mpsl::Peripherals::new(p.RTC0, p.TIMER0, p.TEMP, p.PPI_CH19, p.PPI_CH30, p.PPI_CH31);
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

    let sdc_p = sdc::Peripherals::new(
        p.PPI_CH17, p.PPI_CH18, p.PPI_CH20, p.PPI_CH21, p.PPI_CH22, p.PPI_CH23,
        p.PPI_CH24, p.PPI_CH25, p.PPI_CH26, p.PPI_CH27, p.PPI_CH28, p.PPI_CH29,
    );

    static RNG_CELL: StaticCell<rng::Rng<'static, Async>> = StaticCell::new();
    let rng = RNG_CELL.init(rng::Rng::new(p.RNG, Irqs));

    static SDC_MEM: StaticCell<sdc::Mem<6144>> = StaticCell::new();
    let sdc_mem = SDC_MEM.init(sdc::Mem::new());

    let sdc = unwrap!(build_sdc(sdc_p, rng, mpsl, sdc_mem));

    // Run the BLE peripheral
    run_ble(sdc, led).await;
}

/// Run the BLE stack - separate function to manage lifetimes
async fn run_ble<C: Controller>(controller: C, mut led: Output<'_>) {
    // Build device name
    let name_bytes = build_device_name_bytes();
    let name_str = core::str::from_utf8(&name_bytes).unwrap_or("ECHE");
    info!("Device name: {}", name_str);

    // Create BLE address from device ID
    let node_id = get_node_id();
    let address = Address::random([
        (node_id >> 24) as u8 | 0xC0,
        (node_id >> 16) as u8,
        (node_id >> 8) as u8,
        node_id as u8,
        0x00,
        0x00,
    ]);
    info!("BLE Address: {:?}", address);

    // Initialize BLE host
    let mut resources: HostResources<DefaultPacketPool, CONNECTIONS_MAX, L2CAP_CHANNELS_MAX> = HostResources::new();
    let stack = trouble_host::new(controller, &mut resources).set_random_address(address);
    let Host { mut peripheral, runner, .. } = stack.build();

    info!("Creating Eche GATT server...");

    // Use ManuallyDrop to prevent Drop from being called (we never return)
    let server = ManuallyDrop::new(EcheServer::new_with_config(GapConfig::Peripheral(PeripheralConfig {
        name: name_str,
        appearance: &appearance::UNKNOWN,
    }))
    .unwrap());

    // Set initial characteristic values
    let _ = server.set(&server.eche_service.node_info, &build_node_info());

    info!("Starting BLE tasks...");

    // Run BLE stack and advertising loop concurrently
    let ble_runner = async {
        let mut runner = runner;
        loop {
            if let Err(e) = runner.run().await {
                warn!("BLE runner error: {:?}", defmt::Debug2Format(&e));
            }
        }
    };

    let advertising = async {
        loop {
            match advertise_eche(&name_bytes, &mut peripheral, &server).await {
                Ok(conn) => {
                    info!("Connection established!");
                    led.set_high();

                    let gatt_task = handle_gatt_events(&server, &conn);
                    let notify_task = notification_task(&server, &conn);
                    select(gatt_task, notify_task).await;

                    led.set_low();
                    info!("Connection closed, returning to advertising...");
                }
                Err(e) => {
                    warn!("Advertising error: {:?}", defmt::Debug2Format(&e));
                    Timer::after(Duration::from_secs(1)).await;
                }
            }
        }
    };

    join(ble_runner, advertising).await;
}

async fn advertise_eche<'a, C: Controller>(
    name: &[u8; 9],
    peripheral: &mut Peripheral<'a, C, DefaultPacketPool>,
    server: &'a EcheServer<'a>,
) -> Result<GattConnection<'a, 'a, DefaultPacketPool>, BleHostError<C::Error>> {
    let mut adv_data = [0u8; 31];
    let mut pos = 0;

    // Flags
    adv_data[pos] = 2; pos += 1;
    adv_data[pos] = 0x01; pos += 1;
    adv_data[pos] = LE_GENERAL_DISCOVERABLE | BR_EDR_NOT_SUPPORTED; pos += 1;

    // 16-bit Service UUID (Eche)
    adv_data[pos] = 3; pos += 1;
    adv_data[pos] = 0x03; pos += 1;
    adv_data[pos] = (ECHE_SERVICE_UUID_16BIT & 0xFF) as u8; pos += 1;
    adv_data[pos] = (ECHE_SERVICE_UUID_16BIT >> 8) as u8; pos += 1;

    // Complete Local Name
    adv_data[pos] = (name.len() + 1) as u8; pos += 1;
    adv_data[pos] = 0x09; pos += 1;
    adv_data[pos..pos + name.len()].copy_from_slice(name);
    pos += name.len();

    let advertiser = peripheral
        .advertise(
            &Default::default(),
            Advertisement::ConnectableScannableUndirected {
                adv_data: &adv_data[..pos],
                scan_data: &[],
            },
        )
        .await?;

    info!("Advertising as '{}'...", core::str::from_utf8(name).unwrap_or("ECHE"));

    let conn = advertiser.accept().await?.with_attribute_server(server)?;
    Ok(conn)
}

async fn handle_gatt_events<P: PacketPool>(
    server: &EcheServer<'_>,
    conn: &GattConnection<'_, '_, P>,
) -> Result<(), Error> {
    loop {
        match conn.next().await {
            GattConnectionEvent::Disconnected { reason } => {
                info!("Disconnected: {:?}", reason);
                return Ok(());
            }
            GattConnectionEvent::Gatt { event } => {
                match &event {
                    GattEvent::Read(e) => {
                        if e.handle() == server.eche_service.node_info.handle {
                            info!("Read: node_info");
                        } else if e.handle() == server.eche_service.sync_state.handle {
                            info!("Read: sync_state");
                        } else if e.handle() == server.eche_service.status.handle {
                            info!("Read: status");
                        }
                    }
                    GattEvent::Write(e) => {
                        if e.handle() == server.eche_service.command.handle {
                            info!("Write: command = {:?}", e.data());
                        } else if e.handle() == server.eche_service.sync_data.handle {
                            info!("Write: sync_data ({} bytes)", e.data().len());
                        }
                    }
                    _ => {}
                }
                match event.accept() {
                    Ok(reply) => reply.send().await,
                    Err(e) => warn!("Error sending GATT response: {:?}", e),
                }
            }
            _ => {}
        }
    }
}

async fn notification_task<P: PacketPool>(
    server: &EcheServer<'_>,
    conn: &GattConnection<'_, '_, P>,
) {
    let mut tick = 0u32;
    loop {
        Timer::after(Duration::from_secs(5)).await;
        tick += 5;

        let mut status = [0u8; 8];
        status[0] = 0x01; // connected
        status[4] = (tick >> 24) as u8;
        status[5] = (tick >> 16) as u8;
        status[6] = (tick >> 8) as u8;
        status[7] = tick as u8;

        if server.eche_service.status.notify(conn, &status).await.is_err() {
            info!("Notification failed, connection may be closed");
            break;
        }
    }
}
