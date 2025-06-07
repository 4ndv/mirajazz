use std::{sync::Arc, thread::sleep, time::Duration};

use image::open;
use mirajazz::{
    device::{list_devices, Device},
    error::MirajazzError,
    types::{DeviceInput, ImageFormat, ImageMirroring, ImageMode, ImageRotation},
};

const VID: u16 = 0x0300;
const PID: u16 = 0x1020;

const KEY_COUNT: u8 = 18;

const IMAGE_FORMAT: ImageFormat = ImageFormat {
    mode: ImageMode::JPEG,
    size: (85, 85),
    rotation: ImageRotation::Rot90,
    mirror: ImageMirroring::Both,
};

/// Converts opendeck key index to device key index
fn opendeck_to_device(key: u8) -> u8 {
    if key < KEY_COUNT {
        [12, 9, 6, 3, 0, 15, 13, 10, 7, 4, 1, 16, 14, 11, 8, 5, 2, 17][key as usize]
    } else {
        key
    }
}

/// Converts device key index to opendeck key index
fn device_to_opendeck(key: u8) -> u8 {
    let key = key - 1; // We have to subtract 1 from key index reported by device, because list is shifted by 1

    if key < KEY_COUNT {
        [4, 10, 16, 3, 9, 15, 2, 8, 14, 1, 7, 13, 0, 6, 12, 5, 11, 17][key as usize]
    } else {
        key
    }
}

#[tokio::main]
async fn main() -> Result<(), MirajazzError> {
    println!("Mirajazz example for Ajazz AKP153R");

    for (vid, pid, serial) in list_devices(&[VID]).await? {
        if pid != PID {
            continue;
        }

        println!("Connecting to {:04X}:{:04X}, {}", vid, pid, serial);

        // Connect to the device
        let device = Device::connect(vid, pid, serial, false, false, KEY_COUNT as usize, 0).await?;

        // Print out some info from the device
        println!("Connected to '{}'", device.serial_number().unwrap());

        device.set_brightness(50).await?;
        device.clear_all_button_images().await?;
        // Use image-rs to load an image
        let image = open("examples/test.jpg").unwrap();

        println!("Key count: {}", device.key_count());
        // Write it to the device
        for i in 0..device.key_count() as u8 {
            device
                .set_button_image(opendeck_to_device(i), IMAGE_FORMAT, image.clone())
                .await?;

            sleep(Duration::from_millis(50));

            // Flush
            device.flush().await?;
        }

        let device = Arc::new(device);
        {
            let reader = device.get_reader(|key, _state| {
                println!("Key {}, converted {}", key, device_to_opendeck(key));

                Ok(DeviceInput::NoData)
            });

            loop {
                match reader.read(None).await {
                    Ok(updates) => updates,
                    Err(_) => break,
                };
            }

            drop(reader);
        }

        device.shutdown().await?;
    }

    Ok(())
}
