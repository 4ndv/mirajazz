//! Mirabox K1 Pro (6603:1015).
//!
//! This device needs four things none of the other examples do:
//!
//! * a non-zero HID report ID (`0x04`) on every report — sending `0x00` is silently
//!   ignored, and its incoming reports carry the same prefix, shifting every field by
//!   one byte
//! * synthesised knob releases: it sends a press with no matching release, so without
//!   `with_supports_both_encoder_states(false)` an encoder latches down after the first
//!   press and never fires again
//! * a key order that is not the layout order — each row of the 2x3 grid is addressed
//!   right to left
//! * a payload cap. The device decodes uploads spanning more than one 1024-byte data
//!   report unreliably: they come back torn or half-drawn, and which payloads trip it is
//!   not predictable from the bytes. Identical payloads fail identically through the
//!   vendor's own transport, so it is a device limitation rather than a driver one.
//!   `max_bytes` makes the encoder drop quality until the image fits.

use image::open;
use mirajazz::{
    device::{list_devices, Device, DeviceQuery},
    error::MirajazzError,
    types::{DeviceInput, ImageFormat, ImageMirroring, ImageMode, ImageRotation},
};

const QUERY: DeviceQuery = DeviceQuery::new(65440, 1, 0x6603, 0x1015);

const KEY_COUNT: usize = 6;
const ENCODER_COUNT: usize = 3;
const REPORT_ID: u8 = 0x04;

/// OpenDeck-style key index (row-major from the top left) -> hardware key code.
///
/// `Device::set_button_image` transmits `key + 1`, so entries here are the hardware code
/// minus one. Matches `_IMAGE_KEY_MAP` in the vendor SDK.
const KEY_ORDER: [u8; KEY_COUNT] = [0x05 - 1, 0x03 - 1, 0x01 - 1, 0x06 - 1, 0x04 - 1, 0x02 - 1];

/// 64x64 panels, drawn 1:1 from the panel origin with no scaling, so the size has to be
/// exact: undersized leaves two dark edges, oversized shears.
const IMAGE_FORMAT: ImageFormat = ImageFormat {
    mode: ImageMode::JPEG,
    size: (64, 64),
    rotation: ImageRotation::Rot90,
    mirror: ImageMirroring::None,
    max_bytes: Some(1024),
};

/// Decodes the K1 Pro's input codes.
///
/// Kept separate from the AKP03 decoding because the code spaces overlap with different
/// meanings: `0x25`/`0x30`/`0x31` are knob presses here, not buttons.
fn process_input(input: u8, state: u8) -> Result<DeviceInput, MirajazzError> {
    match input {
        // Six keys with displays. Press is 0x01, release 0x00.
        0x01..=0x06 => {
            let Some(index) = KEY_ORDER.iter().position(|&code| code + 1 == input) else {
                return Err(MirajazzError::BadData);
            };

            let mut buttons = vec![false; KEY_COUNT];
            buttons[index] = state == 0x01;

            println!("Key {} {}", index, if state == 0x01 { "down" } else { "up" });

            Ok(DeviceInput::ButtonStateChange(buttons))
        }

        // Knob presses. Press-only: the release is synthesised by mirajazz.
        0x25 | 0x30 | 0x31 => {
            let encoder = match input {
                0x25 => 0,
                0x30 => 1,
                _ => 2,
            };

            let mut encoders = vec![false; ENCODER_COUNT];
            encoders[encoder] = true;

            println!("Knob {} pressed", encoder);

            Ok(DeviceInput::EncoderStateChange(encoders))
        }

        // Knob rotation. Even code is counter-clockwise, odd is clockwise.
        0x50 | 0x51 | 0x60 | 0x61 | 0x90 | 0x91 => {
            let (encoder, value): (usize, i8) = match input {
                0x50 => (0, -1),
                0x51 => (0, 1),
                0x60 => (1, -1),
                0x61 => (1, 1),
                0x90 => (2, -1),
                _ => (2, 1),
            };

            let mut values = vec![0i8; ENCODER_COUNT];
            values[encoder] = value;

            println!("Knob {} turned {}", encoder, value);

            Ok(DeviceInput::EncoderTwist(values))
        }

        _ => {
            println!("Unrecognized input {:#04x}, state {:#04x}", input, state);

            Ok(DeviceInput::NoData)
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), MirajazzError> {
    println!("Mirajazz example for Mirabox K1 Pro");

    for dev in list_devices(&[QUERY]).await? {
        println!(
            "Connecting to {:04X}:{:04X}, {}",
            dev.vendor_id,
            dev.product_id,
            dev.serial_number.clone().unwrap_or_default()
        );

        let device = Device::connect(&dev, 3, KEY_COUNT, ENCODER_COUNT)
            .await?
            .with_report_id(REPORT_ID)
            .with_supports_both_encoder_states(false);

        println!(
            "Connected to '{}', fw: {:?}",
            device.serial_number(),
            device.firmware_version
        );

        device.set_brightness(50).await?;
        device.clear_all_button_images().await?;

        let image = open("examples/test.jpg").unwrap();

        // Address keys through KEY_ORDER, otherwise images land on the wrong keys
        for index in 0..KEY_COUNT {
            device
                .set_button_image(KEY_ORDER[index], IMAGE_FORMAT, image.clone())
                .await?;
        }

        device.flush().await?;

        println!("Press keys and turn knobs, Ctrl-C to exit");

        let reader = device.get_reader(process_input);

        loop {
            if reader.read(None).await.is_err() {
                break;
            }
        }

        drop(reader);

        device.shutdown().await?;
    }

    Ok(())
}
