use core::cell::RefCell;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use embassy_sync::mutex::Mutex;
use esp_hal_ota::Ota;
use esp_storage::FlashStorage;

// Shared buffer for OTA data transfer
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

    esp_println::println!("OTA initialized successfully");
    Ok(())
}

#[embassy_executor::task]
pub async fn ota_task() {
    let mut ota_opt: Option<Ota<FlashStorage>> = None;

    loop {
        let command = OTA_CHANNEL.receive().await;

        match command {
            OtaCommand::Start { size, crc } => {
                esp_println::println!("Starting OTA update: size={}, crc={}", size, crc);

                match Ota::new(FlashStorage::new()) {
                    Ok(mut ota) => match ota.ota_begin(size, crc) {
                        Ok(_) => {
                            esp_println::println!("OTA begin successful");
                            ota_opt = Some(ota);
                        }
                        Err(e) => {
                            esp_println::println!("OTA begin failed: {:?}", e);
                            ota_opt = None;
                        }
                    },
                    Err(e) => {
                        esp_println::println!("Failed to create OTA instance: {:?}", e);
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
                            esp_println::println!("OTA progress: {}%", progress);

                            if is_complete {
                                esp_println::println!("OTA write complete!");
                            }
                        }
                        Err(e) => {
                            esp_println::println!("OTA write error: {:?}", e);
                            ota_opt = None;
                        }
                    }
                } else {
                    esp_println::println!("OTA not initialized, cannot write chunk");
                }
            }

            OtaCommand::Finish => {
                if let Some(mut ota) = ota_opt.take() {
                    esp_println::println!("Finalizing OTA update...");

                    match ota.ota_flush(false, true) {
                        Ok(_) => {
                            esp_println::println!(
                                "OTA update successful! Rebooting in 2 seconds..."
                            );
                            embassy_time::Timer::after(embassy_time::Duration::from_secs(2)).await;
                            esp_hal::system::software_reset();
                        }
                        Err(e) => {
                            esp_println::println!("OTA flush failed: {:?}", e);
                        }
                    }
                } else {
                    esp_println::println!("OTA not initialized, cannot finish");
                }
            }
        }
    }
}
