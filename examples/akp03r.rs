use image::open;
use mirajazz::{
    device::{list_devices, Device, DeviceQuery},
    error::MirajazzError,
    types::{DeviceInput, ImageFormat, ImageMirroring, ImageMode, ImageRotation},
};

const QUERY: DeviceQuery = DeviceQuery::new(65440, 2, 0x0300, 0x1003);

const IMAGE_FORMAT: ImageFormat = ImageFormat {
    mode: ImageMode::JPEG,
    size: (60, 60),
    rotation: ImageRotation::Rot0,
    mirror: ImageMirroring::None,
};

#[tokio::main]
async fn main() -> Result<(), MirajazzError> {
    println!("Mirajazz example for Ajazz AKP03R");

    for dev in list_devices(&[QUERY]).await? {
        println!(
            "Connecting to {:04X}:{:04X}, {}",
            dev.vendor_id,
            dev.product_id,
            dev.serial_number.clone().unwrap()
        );

        // Connect to the device
        let device = Device::connect(&dev, 2, false, 9, 3).await?;

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
                .set_button_image(i, IMAGE_FORMAT, image.clone())
                .await?;
        }

        // Flush
        device.flush().await?;

        let reader = device.get_reader(|key, state| {
            println!("Key {}, state {}", key, state);

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
