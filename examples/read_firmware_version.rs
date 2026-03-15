use mirajazz::{
    device::{list_devices, Device, DeviceQuery},
    error::MirajazzError,
};

use std::{env, process::exit};

#[tokio::main]
async fn main() -> Result<(), MirajazzError> {
    println!("Read firmware version of device");
    println!(
        "Your device should be already connected and have correct udev rules applied (for Linux)"
    );

    let args: Vec<String> = env::args().collect();

    if args.len() < 3 {
        eprintln!("Usage: cargo run --example read_firmware_version 0000 1111");
        eprintln!("Where 0000 is a vendor_id, and 1111 is a product_id");
        exit(1);
    }

    let vid = u16::from_str_radix(&args[1], 16).unwrap();
    let pid = u16::from_str_radix(&args[2], 16).unwrap();

    let query = DeviceQuery::new(65440, 1, vid, pid);
    let devices = list_devices(&[query]).await?;

    if devices.len() == 0 {
        eprintln!("No connected devices with VID 0x{:X} PID 0x{:X}", vid, pid);
        exit(1);
    }

    for dev in devices {
        println!(
            "Firmware version: {}",
            Device::read_firmware_version(&dev).await?
        );
    }

    Ok(())
}
