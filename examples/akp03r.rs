use std::{process::exit, sync::Arc, time::Duration};

use image::open;
use mirajazz::{
    device::{list_devices, new_hidapi, Device},
    types::{DeviceInput, ImageFormat, ImageMirroring, ImageMode, ImageRotation},
};

const VID: u16 = 0x0300;
const PID: u16 = 0x1003;

const IMAGE_FORMAT: ImageFormat = ImageFormat {
    mode: ImageMode::JPEG,
    size: (60, 60),
    rotation: ImageRotation::Rot0,
    mirror: ImageMirroring::None,
};

fn main() {
    println!("Mirajazz example for Ajazz AKP03R");

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
        let device =
            Device::connect(&hidapi, vid, pid, &serial, true, 9, 3).expect("Failed to connect");
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
                .set_button_image(i, IMAGE_FORMAT, image.clone())
                .unwrap();
        }

        // Flush
        device.flush().unwrap();

        let device = Arc::new(device);
        {
            let reader = device.get_reader();

            loop {
                match reader.read(
                    Some(Duration::from_secs_f64(100.0)),
                    |key, state| {
                        println!("Key {}, state {}", key, state);

                        Ok(DeviceInput::NoData)
                    },
                    false,
                ) {
                    Ok(updates) => updates,
                    Err(_) => break,
                };
            }

            drop(reader);
        }

        device.shutdown().ok();
    }
}
