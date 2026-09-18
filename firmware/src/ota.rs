use core::cell::RefCell;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use embassy_sync::mutex::Mutex;
use esp_hal_ota::Ota;
use esp_storage::FlashStorage;

// FIXED: Increased buffer to 8KB to match working example
pub static OTA_BUFFER: Mutex<CriticalSectionRawMutex, RefCell<[u8; 8192]>> =
    Mutex::new(RefCell::new([0u8; 8192]));

pub static OTA_CHANNEL: Channel<CriticalSectionRawMutex, OtaCommand, 8> = Channel::new();

#[derive(Clone, Copy, Debug)]
pub enum OtaCommand {
    Start { size: u32, crc: u32 },
    WriteChunk { len: usize },
    Finish,
}

#[derive(Clone, Copy, Debug)]
pub enum OtaError {
    InitFailed,
    BeginFailed,
    WriteFailed,
    FlushFailed,
    VerifyFailed,
}

/// Initialize OTA and mark the current app as valid
pub fn ota_init() -> Result<(), ()> {
    let mut ota = Ota::new(FlashStorage::new()).map_err(|_| ())?;

    // Mark current partition as valid to prevent rollback
    // Note: This will fail if running from factory partition (which is expected)
    let _ = ota.ota_mark_app_valid();

    defmt::info!("OTA initialized successfully");
    Ok(())
}

#[embassy_executor::task]
pub async fn ota_task() {
    let mut ota_opt: Option<Ota<FlashStorage>> = None;

    loop {
        let command = OTA_CHANNEL.receive().await;

        match command {
            OtaCommand::Start { size, crc } => {
                defmt::info!("Starting OTA update: size={}, crc={}", size, crc);

                match Ota::new(FlashStorage::new()) {
                    Ok(mut ota) => match ota.ota_begin(size, crc) {
                        Ok(_) => {
                            defmt::info!("OTA begin successful");
                            ota_opt = Some(ota);
                        }
                        Err(e) => {
                            defmt::error!("OTA begin failed: {}", defmt::Debug2Format(&e));
                            ota_opt = None;
                        }
                    },
                    Err(e) => {
                        defmt::error!("Failed to create OTA instance: {}", defmt::Debug2Format(&e));
                        ota_opt = None;
                    }
                }
            }

            OtaCommand::WriteChunk { len } => {
                if let Some(ref mut ota) = ota_opt {
                    // Access the shared buffer
                    let buffer_guard = OTA_BUFFER.lock().await;
                    let buffer = buffer_guard.borrow();

                    match ota.ota_write_chunk(&buffer[..len]) {
                        Ok(is_complete) => {
                            let progress = (ota.get_ota_progress() * 100.0) as u8;
                            defmt::info!("OTA progress: {}%", progress);

                            if is_complete {
                                defmt::info!("OTA write complete!");
                            }
                        }
                        Err(e) => {
                            defmt::error!("OTA write error: {}", defmt::Debug2Format(&e));
                            ota_opt = None;
                        }
                    }
                } else {
                    defmt::warn!("OTA not initialized, cannot write chunk");
                }
            }

            OtaCommand::Finish => {
                if let Some(mut ota) = ota_opt.take() {
                    defmt::info!("Finalizing OTA update...");

                    match ota.ota_flush(false, true) {
                        Ok(_) => {
                            defmt::info!("OTA update successful! Rebooting in 2 seconds...");
                            embassy_time::Timer::after(embassy_time::Duration::from_secs(2)).await;
                            esp_hal::system::software_reset();
                        }
                        Err(e) => {
                            defmt::error!("OTA flush failed: {}", defmt::Debug2Format(&e));
                        }
                    }
                } else {
                    defmt::warn!("OTA not initialized, cannot finish");
                }
            }
        }
    }
}
