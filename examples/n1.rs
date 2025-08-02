use image::open;
use mirajazz::{
    device::{list_devices, Device, DeviceQuery},
    error::MirajazzError,
    types::{DeviceInput, ImageFormat, ImageMirroring, ImageMode, ImageRotation},
};
use std::{thread::sleep, time::Duration};

const QUERY: DeviceQuery = DeviceQuery::new(65440, 2, 0x6603, 0x1000);

#[repr(u8)]
enum N1Mode {
    Keyboard = 1,
    Calculator = 2,
    Software = 3,
}

const KEY_COUNT: u8 = 18;

fn image_format_for_key(key: u8) -> ImageFormat {
    if key >= 15 {
        TOP_ROW_IMAGE_FORMAT
    } else {
        IMAGE_FORMAT
    }
}

const IMAGE_FORMAT: ImageFormat = ImageFormat {
    mode: ImageMode::JPEG,
    size: (96, 96),
    rotation: ImageRotation::Rot0,
    mirror: ImageMirroring::None,
};

const TOP_ROW_IMAGE_FORMAT: ImageFormat = ImageFormat {
    mode: ImageMode::JPEG,
    size: (64, 64),
    rotation: ImageRotation::Rot0,
    mirror: ImageMirroring::None,
};


#[tokio::main]
async fn main() -> Result<(), MirajazzError> {
    println!("Mirajazz example for MiraBox N1");

    for dev in list_devices(&[QUERY]).await? {
        println!(
            "Connecting to {:04X}:{:04X}, {}",
            dev.vendor_id,
            dev.product_id,
            dev.serial_number.clone().unwrap()
        );

        // Connect to the device
        let device = Device::connect(&dev, true, true, KEY_COUNT as usize, 0).await?;
        device.set_mode(N1Mode::Software as u8).await?;
        sleep(Duration::from_millis(50));

        // Print out some info from the device
        println!("Connected to '{}'", device.serial_number());

        device.set_brightness(50).await?;
        device.clear_all_button_images().await?;
        // Use image-rs to load an image
        let image = open("examples/test.jpg").unwrap();

        println!("Key count: {}", device.key_count());
        // Write it to the device
        for i in 0..device.key_count() as u8 {
            device
                .set_button_image(i, image_format_for_key(i), image.clone())
                .await?;

            // Flush
            device.flush().await?;
        }

        let reader = device.get_reader(|key, state| {
            println!("Key {}: {state}", key);

            Ok(DeviceInput::NoData)
        });

        loop {
            match reader.read(None).await {
                Ok(updates) => updates,
                Err(_) => break,
            };
        }

        drop(reader);

        device.shutdown().await?;
    }

    Ok(())
}
