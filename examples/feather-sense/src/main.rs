//! Eche SOS Beacon for Adafruit Feather nRF52840 Sense
//!
//! - User switch triggers SOS
//! - Red LED = SOS active
//! - Blue LED = BLE connected
//! - Static position: 1423 Beatie AVE SW, Atlanta GA 30310

#![no_std]
#![no_main]

use core::mem;
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use defmt::{info, warn, Debug2Format};
use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_nrf::gpio::{Input, Level, Output, OutputDrive, Pull};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::signal::Signal;
use embassy_time::{Duration, Instant, Timer, with_timeout};
use nrf_softdevice::ble::advertisement_builder::{
    AdvertisementDataType, Flag, LegacyAdvertisementBuilder, ServiceList, ServiceUuid16,
};
use nrf_softdevice::ble::gatt_server;
use nrf_softdevice::ble::peripheral;
use nrf_softdevice::{raw, Softdevice};
use panic_probe as _;
use portable_atomic::AtomicU64;

// =============================================================================
// Constants
// =============================================================================

/// Eche service UUID 16-bit (matches Android EcheBtle.kt)
const HIVE_SERVICE_UUID_16: u16 = 0xF47A;

/// Device name for SD config
const DEVICE_NAME: &str = "BFS";

/// WEARTAK Mesh Encryption Secret (derived from genesis via BLAKE3)
#[allow(dead_code)]
const ENCRYPTION_SECRET: [u8; 32] = [
    0xF0, 0x1D, 0x46, 0x74, 0xCD, 0x11, 0xD3, 0xB1,
    0x70, 0x02, 0x37, 0x8D, 0x95, 0xD0, 0x9F, 0xB9,
    0xF2, 0xC3, 0x2C, 0x2E, 0x8C, 0x3E, 0xA4, 0xD4,
    0x62, 0x19, 0x0B, 0x50, 0x63, 0x9F, 0x82, 0x31,
];

/// WEARTAK Beacon Key Base (for encrypted advertisements)
#[allow(dead_code)]
const BEACON_KEY: [u8; 32] = [
    0x81, 0x72, 0x5C, 0x7B, 0x45, 0x26, 0x52, 0x12,
    0x94, 0x51, 0xAE, 0x19, 0x93, 0x44, 0x95, 0x01,
    0x83, 0x51, 0x36, 0x4A, 0xDF, 0xDD, 0xB1, 0xF0,
    0x10, 0x29, 0x66, 0x2E, 0x39, 0xBB, 0x1F, 0x02,
];

/// Static position: 1423 Beatie AVE SW, Atlanta GA 30310
const STATIC_LATITUDE: f32 = 33.7384;
const STATIC_LONGITUDE: f32 = -84.4168;

/// Build Eche beacon service data (10 bytes)
/// Format: version|caps_hi, caps_lo, node_id[4], hierarchy, battery, seq[2]
fn build_beacon(seq: u16, battery: u8) -> [u8; 10] {
    let mut beacon = [0u8; 10];
    // Version 1 (4 bits) | Capabilities high 0x0 (4 bits)
    beacon[0] = 0x10; // Version 1, no special caps
    // Capabilities low
    beacon[1] = 0x00;
    // Node ID (big-endian)
    beacon[2..6].copy_from_slice(&NODE_ID.load(Ordering::Relaxed).to_be_bytes());
    // Hierarchy: 0 = Platform (leaf node)
    beacon[6] = 0x00;
    // Battery
    beacon[7] = battery;
    // Sequence (big-endian)
    beacon[8..10].copy_from_slice(&seq.to_be_bytes());
    beacon
}

// =============================================================================
// Shared State
// =============================================================================

static NODE_ID: AtomicU32 = AtomicU32::new(0);
static SOS_ACTIVE: AtomicBool = AtomicBool::new(false);
static SOS_TIMESTAMP: AtomicU64 = AtomicU64::new(0);
static SOS_ACKS: AtomicU32 = AtomicU32::new(0);
static PEER_COUNT: AtomicU32 = AtomicU32::new(0);
static LED_UPDATE: Signal<CriticalSectionRawMutex, ()> = Signal::new();

// =============================================================================
// GATT Server
// =============================================================================

#[nrf_softdevice::gatt_server]
struct EcheServer {
    hive: EcheService,
}

#[nrf_softdevice::gatt_service(uuid = "f47ac10b-58cc-4372-a567-0e02b2c3d479")]
struct EcheService {
    #[characteristic(uuid = "f47a0003-58cc-4372-a567-0e02b2c3d479", read, write, notify)]
    document: [u8; 128],

    #[characteristic(uuid = "f47a0010-58cc-4372-a567-0e02b2c3d479", read, notify)]
    sensor_data: [u8; 8],

    #[characteristic(uuid = "f47a0011-58cc-4372-a567-0e02b2c3d479", read)]
    position: [u8; 8],

    #[characteristic(uuid = "f47a0012-58cc-4372-a567-0e02b2c3d479", read, write, notify)]
    emergency: [u8; 16],
}

// =============================================================================
// Tasks
// =============================================================================

#[embassy_executor::task]
async fn softdevice_task(sd: &'static Softdevice) -> ! {
    sd.run().await
}

#[embassy_executor::task]
async fn button_task(btn: Input<'static>) {
    info!("Button task started - press user switch for SOS");

    loop {
        if btn.is_low() {
            Timer::after(Duration::from_millis(50)).await;
            if btn.is_low() {
                let active = SOS_ACTIVE.load(Ordering::SeqCst);
                if active {
                    SOS_ACTIVE.store(false, Ordering::SeqCst);
                    SOS_ACKS.store(0, Ordering::SeqCst);
                    info!("SOS cancelled");
                } else {
                    SOS_ACTIVE.store(true, Ordering::SeqCst);
                    SOS_TIMESTAMP.store(Instant::now().as_millis(), Ordering::SeqCst);
                    SOS_ACKS.store(0, Ordering::SeqCst);
                    info!("SOS ACTIVATED!");
                }
                LED_UPDATE.signal(());
                while btn.is_low() {
                    Timer::after(Duration::from_millis(10)).await;
                }
            }
        }
        Timer::after(Duration::from_millis(50)).await;
    }
}

#[embassy_executor::task]
async fn led_task(mut red_led: Output<'static>, mut blue_led: Output<'static>) {
    info!("LED task started");

    loop {
        let sos = SOS_ACTIVE.load(Ordering::SeqCst);
        let connected = PEER_COUNT.load(Ordering::SeqCst) > 0;

        // Blue LED = connected
        if connected {
            blue_led.set_high();
        } else {
            blue_led.set_low();
        }

        // Red LED = SOS status
        if sos {
            // SOS active: fast blink (100ms on/off)
            red_led.set_high();
            Timer::after(Duration::from_millis(100)).await;
            red_led.set_low();
            Timer::after(Duration::from_millis(100)).await;
        } else {
            // Normal: slow heartbeat (200ms on, 2s off)
            red_led.set_high();
            Timer::after(Duration::from_millis(200)).await;
            red_led.set_low();
            Timer::after(Duration::from_millis(2000)).await;
        }
    }
}

// =============================================================================
// Helpers
// =============================================================================

fn encode_position() -> [u8; 8] {
    let mut buf = [0u8; 8];
    buf[0..4].copy_from_slice(&STATIC_LATITUDE.to_le_bytes());
    buf[4..8].copy_from_slice(&STATIC_LONGITUDE.to_le_bytes());
    buf
}

fn encode_emergency() -> [u8; 16] {
    let mut buf = [0u8; 16];
    buf[0] = if SOS_ACTIVE.load(Ordering::SeqCst) { 1 } else { 0 };
    buf[1..9].copy_from_slice(&SOS_TIMESTAMP.load(Ordering::SeqCst).to_le_bytes());
    buf[9..13].copy_from_slice(&SOS_ACKS.load(Ordering::SeqCst).to_le_bytes());
    buf
}

fn encode_sensor_data() -> [u8; 8] {
    // Mock sensor data - TODO: read actual sensors
    let mut buf = [0u8; 8];
    buf[0..2].copy_from_slice(&225i16.to_le_bytes()); // 22.5C
    buf[2] = 45; // 45% humidity
    buf[3..5].copy_from_slice(&1013u16.to_le_bytes()); // 1013 hPa
    buf[5] = 85; // 85% battery
    buf
}

// =============================================================================
// Main
// =============================================================================

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    // Read FICR for unique NODE_ID (goes in beacon service data)
    let node_id = unsafe { core::ptr::read_volatile(0x10000060 as *const u32) };
    NODE_ID.store(node_id, Ordering::SeqCst);

    info!("ECHE-SENSE starting... NodeID: {:08X}", node_id);
    info!("Position: {}, {}", STATIC_LATITUDE, STATIC_LONGITUDE);

    // Use default embassy config like adafruit-clue example
    let p = embassy_nrf::init(Default::default());

    // Feather Sense pinout
    let btn = Input::new(p.P1_02, Pull::Up);      // User switch
    let red_led = Output::new(p.P1_09, Level::Low, OutputDrive::Standard);   // Red LED (P1.09, active HIGH)
    let blue_led = Output::new(p.P1_10, Level::Low, OutputDrive::Standard);  // Blue LED (P1.10, active HIGH)

    // SoftDevice config
    let sd_config = nrf_softdevice::Config {
        clock: Some(raw::nrf_clock_lf_cfg_t {
            source: raw::NRF_CLOCK_LF_SRC_RC as u8,
            rc_ctiv: 16,
            rc_temp_ctiv: 2,
            accuracy: raw::NRF_CLOCK_LF_ACCURACY_500_PPM as u8,
        }),
        conn_gap: Some(raw::ble_gap_conn_cfg_t {
            conn_count: 2,
            event_length: 24,
        }),
        conn_gatt: Some(raw::ble_gatt_conn_cfg_t { att_mtu: 128 }),
        gatts_attr_tab_size: Some(raw::ble_gatts_cfg_attr_tab_size_t {
            attr_tab_size: 2048,
        }),
        gap_role_count: Some(raw::ble_gap_cfg_role_count_t {
            adv_set_count: 1,
            periph_role_count: 2,
            central_role_count: 0,
            central_sec_count: 0,
            _bitfield_1: raw::ble_gap_cfg_role_count_t::new_bitfield_1(0),
        }),
        gap_device_name: Some(raw::ble_gap_cfg_device_name_t {
            p_value: DEVICE_NAME.as_ptr() as *const u8 as *mut u8,
            current_len: DEVICE_NAME.len() as u16,
            max_len: DEVICE_NAME.len() as u16,
            write_perm: unsafe { mem::zeroed() },
            _bitfield_1: raw::ble_gap_cfg_device_name_t::new_bitfield_1(
                raw::BLE_GATTS_VLOC_STACK as u8,
            ),
        }),
        ..Default::default()
    };

    let sd = Softdevice::enable(&sd_config);
    info!("SoftDevice enabled");

    let server = EcheServer::new(sd).unwrap();

    server.hive.position_set(&encode_position()).unwrap();
    server.hive.emergency_set(&encode_emergency()).unwrap();
    server.hive.sensor_data_set(&encode_sensor_data()).unwrap();

    // Startup blink - 3 quick red blinks to show firmware started
    let mut red_led = red_led;
    let mut blue_led = blue_led;
    for _ in 0..3 {
        red_led.set_high();
        Timer::after(Duration::from_millis(100)).await;
        red_led.set_low();
        Timer::after(Duration::from_millis(100)).await;
    }

    spawner.spawn(softdevice_task(sd).unwrap());

    // Build unique device name: BFS-XXXX
    let suffix = (node_id & 0xFFFF) as u16;
    let hex = b"0123456789ABCDEF";
    let device_name: [u8; 8] = [
        b'B', b'F', b'S', b'-',
        hex[((suffix >> 12) & 0xF) as usize],
        hex[((suffix >> 8) & 0xF) as usize],
        hex[((suffix >> 4) & 0xF) as usize],
        hex[(suffix & 0xF) as usize],
    ];
    let device_name_str = unsafe { core::str::from_utf8_unchecked(&device_name) };
    info!("BLE ready, advertising as {}", device_name_str);

    let mut adv_seq: u16 = 0;

    loop {
        // Build beacon with current sequence and battery
        let _beacon = build_beacon(adv_seq, 85); // 85% battery mock (for future GATT use)
        adv_seq = adv_seq.wrapping_add(1);

        // Build service data: uuid(2) + node_id(4) = 6 bytes
        let mut svc_data = [0u8; 6];
        svc_data[0..2].copy_from_slice(&HIVE_SERVICE_UUID_16.to_le_bytes());
        svc_data[2..6].copy_from_slice(&NODE_ID.load(Ordering::Relaxed).to_be_bytes());

        let adv_data = LegacyAdvertisementBuilder::new()
            .flags(&[Flag::GeneralDiscovery, Flag::LE_Only])
            .services_16(ServiceList::Complete, &[ServiceUuid16::from_u16(HIVE_SERVICE_UUID_16)])
            .raw(AdvertisementDataType::SERVICE_DATA_16, &svc_data)
            .build();

        // Put unique name in scan response: BFS-XXXX (8 chars)
        let suffix = (NODE_ID.load(Ordering::Relaxed) & 0xFFFF) as u16;
        let hex = b"0123456789ABCDEF";
        let name_buf: [u8; 8] = [
            b'B', b'F', b'S', b'-',
            hex[((suffix >> 12) & 0xF) as usize],
            hex[((suffix >> 8) & 0xF) as usize],
            hex[((suffix >> 4) & 0xF) as usize],
            hex[(suffix & 0xF) as usize],
        ];
        let name_str = unsafe { core::str::from_utf8_unchecked(&name_buf) };
        let scan_data = LegacyAdvertisementBuilder::new()
            .full_name(name_str)
            .build();
        let config = peripheral::Config::default();

        let adv = peripheral::ConnectableAdvertisement::ScannableUndirected {
            adv_data: &adv_data,
            scan_data: &scan_data,
        };

        // Use timeout so other tasks can run while advertising
        let adv_result = with_timeout(
            Duration::from_secs(5),
            peripheral::advertise_connectable(sd, adv, &config)
        ).await;

        match adv_result {
            Ok(Ok(conn)) => {
                info!("Connected!");
                PEER_COUNT.fetch_add(1, Ordering::SeqCst);
                LED_UPDATE.signal(());

                let res = gatt_server::run(&conn, &server, |event| match event {
                    EcheServerEvent::Eche(e) => match e {
                        EcheServiceEvent::DocumentWrite(data) => {
                            info!("Document: {} bytes", data.len());
                        }
                        EcheServiceEvent::EmergencyWrite(data) => {
                            if data.len() >= 13 && SOS_ACTIVE.load(Ordering::SeqCst) {
                                SOS_ACKS.fetch_add(1, Ordering::SeqCst);
                                info!("ACK received! Total: {}", SOS_ACKS.load(Ordering::SeqCst));
                                LED_UPDATE.signal(());
                            }
                        }
                        EcheServiceEvent::EmergencyCccdWrite { notifications } => {
                            if notifications && SOS_ACTIVE.load(Ordering::SeqCst) {
                                let _ = server.hive.emergency_notify(&conn, &encode_emergency());
                            }
                        }
                        _ => {}
                    },
                })
                .await;

                info!("Disconnected: {:?}", Debug2Format(&res));
                PEER_COUNT.fetch_sub(1, Ordering::SeqCst);
                LED_UPDATE.signal(());
            }
            Ok(Err(e)) => {
                warn!("Advertise error: {:?}", Debug2Format(&e));
            }
            Err(_) => {
                // Timeout - check button and update LEDs

                // Check button (debounced)
                if btn.is_low() {
                    Timer::after(Duration::from_millis(50)).await;
                    if btn.is_low() {
                        let active = SOS_ACTIVE.load(Ordering::SeqCst);
                        SOS_ACTIVE.store(!active, Ordering::SeqCst);
                        if !active {
                            SOS_TIMESTAMP.store(Instant::now().as_millis(), Ordering::SeqCst);
                            info!("SOS ACTIVATED!");
                        } else {
                            info!("SOS cancelled");
                        }
                        // Wait for button release
                        while btn.is_low() {
                            Timer::after(Duration::from_millis(10)).await;
                        }
                    }
                }
            }
        }

        // Update LEDs based on state
        let sos = SOS_ACTIVE.load(Ordering::SeqCst);
        let connected = PEER_COUNT.load(Ordering::SeqCst) > 0;

        if connected {
            blue_led.set_high();
        } else {
            blue_led.set_low();
        }

        if sos {
            // Fast blink for SOS
            red_led.set_high();
        } else {
            // Heartbeat - toggle based on sequence
            if adv_seq % 2 == 0 {
                red_led.set_high();
            } else {
                red_led.set_low();
            }
        }
    }
}
