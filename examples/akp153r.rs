use std::{process::exit, sync::Arc, thread::sleep, time::Duration};

use image::open;
use mirajazz::{
    device::{list_devices, new_hidapi, Device},
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

fn main() {
    println!("Mirajazz example for Ajazz AKP153R");

    let hidapi = match new_hidapi() {
        Ok(hidapi) => hidapi,
        Err(e) => {
            eprintln!("Failed to create HidApi instance: {}", e);
            exit(1);
        }
    };

    for (vid, pid, serial) in list_devices(&hidapi, &[VID]) {
        if pid != PID {
            continue;
        }

        println!("Connecting to {:04X}:{:04X}, {}", vid, pid, serial);

        // Connect to the device
        let device = Device::connect(
            &hidapi,
            vid,
            pid,
            &serial,
            false,
            false,
            KEY_COUNT as usize,
            0,
        )
        .expect("Failed to connect");
        // Print out some info from the device
        println!(
            "Connected to '{}' with version '{}'",
            device.serial_number().unwrap(),
            device.firmware_version().unwrap()
        );

        device.set_brightness(50).unwrap();
        device.clear_all_button_images().unwrap();
        // Use image-rs to load an image
        let image = open("examples/test.jpg").unwrap();

        println!("Key count: {}", device.key_count());
        // Write it to the device
        for i in 0..device.key_count() as u8 {
            device
                .set_button_image(opendeck_to_device(i), IMAGE_FORMAT, image.clone())
                .unwrap();

            sleep(Duration::from_millis(50));

            // Flush
            device.flush().unwrap();
        }

        let device = Arc::new(device);
        {
            let reader = device.get_reader();

            loop {
                match reader.read(Some(Duration::from_secs_f64(100.0)), |key, _state| {
                    println!("Key {}, converted {}", key, device_to_opendeck(key));

                    Ok(DeviceInput::NoData)
                }) {
                    Ok(updates) => updates,
                    Err(_) => break,
                };
            }

            drop(reader);
        }

        device.shutdown().ok();
    }
}
