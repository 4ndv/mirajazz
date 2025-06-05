use std::{
    cell::RefCell,
    collections::HashSet,
    str::{from_utf8, Utf8Error},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

use async_hid::{AsyncHidRead, AsyncHidWrite, Device as HidDevice, DeviceReaderWriter, HidBackend};
use async_io::Timer;
use futures_lite::{FutureExt, StreamExt};
use image::DynamicImage;
use tokio::sync::Mutex;

use crate::{
    error::MirajazzError,
    images::convert_image_with_format,
    state::{DeviceState, DeviceStateReader},
    types::{DeviceInput, ImageFormat},
};

/// Creates an instance of the async-hid backend
///
/// Can be used if you don't want to link async-hid crate into your project
pub fn new_hid_backend() -> HidBackend {
    HidBackend::default()
}

/// Returns a list of devices as (Kind, Serial Number) that could be found using HidApi.
///
/// **WARNING:** To refresh the list, use [refresh_device_list]
pub async fn list_devices(vids: &[u16]) -> Result<HashSet<(u16, u16, String)>, MirajazzError> {
    let devices = HidBackend::default()
        .enumerate()
        .await?
        .map(HidDevice::to_device_info)
        .filter_map(|d| {
            if !vids.contains(&d.vendor_id) {
                return None;
            }

            if let Some(serial) = d.serial_number {
                Some((d.vendor_id, d.product_id, serial.to_string()))
            } else {
                None
            }
        })
        .collect::<HashSet<_>>()
        .await;

    Ok(devices)
}

/// Extracts string from byte array, removing \0 symbols
pub fn extract_str(bytes: &[u8]) -> Result<String, Utf8Error> {
    Ok(from_utf8(bytes)?.replace('\0', "").to_string())
}

struct ImageCache {
    key: u8,
    image_data: Vec<u8>,
}

/// Interface for a device
pub struct Device {
    /// Vendor ID of the device
    pub vid: u16,
    /// Product ID of the device
    pub pid: u16,
    /// Serial number
    pub serial_number: String,
    /// Use v2 hacks
    is_v2: bool,
    /// Emits two events for buttons or not
    supports_both_states: bool,
    /// Number of keys
    key_count: usize,
    /// Number of encoders
    encoder_count: usize,
    /// Packet size
    packet_size: usize,
    /// Connected HIDDevice
    hid_device: HidDevice,
    /// Device reader/writer
    reader_writer: RefCell<DeviceReaderWriter>,
    /// Temporarily cache the image before sending it to the device
    image_cache: Mutex<Vec<ImageCache>>,
    /// Device needs to be initialized
    initialized: AtomicBool,
}

/// Static functions of the struct
impl Device {
    /// Attempts to connect to the device
    pub async fn connect(
        vid: u16,
        pid: u16,
        serial: String,
        is_v2: bool,
        supports_both_states: bool,
        key_count: usize,
        encoder_count: usize,
    ) -> Result<Device, MirajazzError> {
        let hid_device = HidBackend::default()
            .enumerate()
            .await?
            .find(|d| {
                d.vendor_id == vid && d.product_id == pid && d.serial_number == Some(serial.clone())
            })
            .await;

        if let Some(hid_device) = hid_device {
            let reader_writer = hid_device.open().await?;

            Ok(Device {
                vid,
                pid,
                serial_number: serial.to_string(),
                is_v2,
                supports_both_states,
                key_count,
                encoder_count,
                hid_device,
                reader_writer: RefCell::new(reader_writer),
                packet_size: if is_v2 { 1024 } else { 512 },
                image_cache: Mutex::new(vec![]),
                initialized: false.into(),
            })
        } else {
            Err(MirajazzError::DeviceNotFoundError)
        }
    }
}

/// Instance methods of the struct
impl Device {
    /// Returns key count
    pub fn key_count(&self) -> usize {
        self.key_count
    }

    /// Returns encoder count
    pub fn encoder_count(&self) -> usize {
        self.encoder_count
    }

    /// Returns serial number of the device
    pub fn serial_number(&self) -> Option<String> {
        self.hid_device.serial_number.clone()
    }

    /// Initializes the device
    async fn initialize(&self) -> Result<(), MirajazzError> {
        if self.initialized.load(Ordering::Acquire) {
            return Ok(());
        }

        self.initialized.store(true, Ordering::Release);

        let mut buf = vec![0x00, 0x43, 0x52, 0x54, 0x00, 0x00, 0x44, 0x49, 0x53];
        self.write_extended_data(&mut buf).await?;

        let mut buf = vec![
            0x00, 0x43, 0x52, 0x54, 0x00, 0x00, 0x4c, 0x49, 0x47, 0x00, 0x00, 0x00, 0x00,
        ];
        self.write_extended_data(&mut buf).await?;

        Ok(())
    }

    /// Returns value of `supports_both_states`
    pub fn supports_both_states(&self) -> bool {
        self.supports_both_states
    }

    /// Reads current input state from the device and calls provided function for processing
    pub async fn read_input(
        &self,
        timeout: Option<Duration>,
        process_input: fn(u8, u8) -> Result<DeviceInput, MirajazzError>,
    ) -> Result<DeviceInput, MirajazzError> {
        self.initialize().await?;

        let data = if timeout.is_some() {
            self.read_data_with_timeout(512, timeout.unwrap()).await?
        } else {
            Some(self.read_data(512).await?)
        };

        if data.is_none() {
            return Ok(DeviceInput::NoData);
        }

        let data = data.unwrap();

        if data[0] == 0 {
            return Ok(DeviceInput::NoData);
        }

        let state = if self.supports_both_states() {
            data[10]
        } else {
            0x1u8
        };

        Ok(process_input(data[9], state)?)
    }

    /// Resets the device
    pub async fn reset(&self) -> Result<(), MirajazzError> {
        self.initialize().await?;

        self.set_brightness(100).await?;
        self.clear_all_button_images().await?;

        Ok(())
    }

    /// Sets brightness of the device, value range is 0 - 100
    pub async fn set_brightness(&self, percent: u8) -> Result<(), MirajazzError> {
        self.initialize().await?;

        let percent = percent.clamp(0, 100);

        let mut buf = vec![
            0x00, 0x43, 0x52, 0x54, 0x00, 0x00, 0x4c, 0x49, 0x47, 0x00, 0x00, percent,
        ];

        self.write_extended_data(&mut buf).await?;

        Ok(())
    }

    async fn send_image(&self, key: u8, image_data: &[u8]) -> Result<(), MirajazzError> {
        let mut buf = vec![
            0x00,
            0x43,
            0x52,
            0x54,
            0x00,
            0x00,
            0x42,
            0x41,
            0x54,
            0x00,
            0x00,
            (image_data.len() >> 8) as u8,
            image_data.len() as u8,
            key + 1,
        ];

        self.write_extended_data(&mut buf).await?;

        self.write_image_data_reports(image_data).await?;

        Ok(())
    }

    /// Writes image data to device, changes must be flushed with `.flush()` before
    /// they will appear on the device!
    pub async fn write_image(&self, key: u8, image_data: &[u8]) -> Result<(), MirajazzError> {
        let cache_entry = ImageCache {
            key,
            image_data: image_data.to_vec(), // Convert &[u8] to Vec<u8>
        };

        self.image_cache.lock().await.push(cache_entry);

        Ok(())
    }

    /// Sets button's image to blank, changes must be flushed with `.flush()` before
    /// they will appear on the device!
    pub async fn clear_button_image(&self, key: u8) -> Result<(), MirajazzError> {
        self.initialize().await?;

        let mut buf = vec![
            0x00,
            0x43,
            0x52,
            0x54,
            0x00,
            0x00,
            0x43,
            0x4c,
            0x45,
            0x00,
            0x00,
            0x00,
            if key == 0xff { 0xff } else { key + 1 },
        ];

        self.write_extended_data(&mut buf).await?;

        Ok(())
    }

    /// Sets blank images to every button, changes must be flushed with `.flush()` before
    /// they will appear on the device!
    pub async fn clear_all_button_images(&self) -> Result<(), MirajazzError> {
        self.initialize().await?;

        self.clear_button_image(0xFF).await?;

        if self.is_v2 {
            // Mirabox "v2" requires STP to commit clearing the screen
            let mut buf = vec![0x00, 0x43, 0x52, 0x54, 0x00, 0x00, 0x53, 0x54, 0x50];

            self.write_extended_data(&mut buf).await?;
        }

        Ok(())
    }

    /// Sets specified button's image, changes must be flushed with `.flush()` before
    /// they will appear on the device!
    pub async fn set_button_image(
        &self,
        key: u8,
        image_format: ImageFormat,
        image: DynamicImage,
    ) -> Result<(), MirajazzError> {
        self.initialize().await?;

        let image_data = convert_image_with_format(image_format, image).await?;

        self.write_image(key, &image_data).await?;

        Ok(())
    }

    /// Sleeps the device
    pub async fn sleep(&self) -> Result<(), MirajazzError> {
        self.initialize().await?;

        let mut buf = vec![0x00, 0x43, 0x52, 0x54, 0x00, 0x00, 0x48, 0x41, 0x4e];
        self.write_extended_data(&mut buf).await?;

        Ok(())
    }

    /// Make periodic events to the device, to keep it alive
    pub async fn keep_alive(&self) -> Result<(), MirajazzError> {
        self.initialize().await?;

        let mut buf = vec![
            0x00, 0x43, 0x52, 0x54, 0x00, 0x00, 0x43, 0x4F, 0x4E, 0x4E, 0x45, 0x43, 0x54,
        ];

        self.write_extended_data(&mut buf).await?;

        Ok(())
    }

    /// Shutdown the device
    pub async fn shutdown(&self) -> Result<(), MirajazzError> {
        self.initialize().await?;

        let mut buf = vec![
            0x00, 0x43, 0x52, 0x54, 0x00, 0x00, 0x43, 0x4c, 0x45, 0x00, 0x00, 0x44, 0x43,
        ];
        self.write_extended_data(&mut buf).await?;

        let mut buf = vec![0x00, 0x43, 0x52, 0x54, 0x00, 0x00, 0x48, 0x41, 0x4E];
        self.write_extended_data(&mut buf).await?;

        Ok(())
    }

    /// Flushes the button's image to the device
    pub async fn flush(&self) -> Result<(), MirajazzError> {
        let mut cache = self.image_cache.lock().await;

        self.initialize().await?;

        if cache.is_empty() {
            return Ok(());
        }

        for image in cache.iter() {
            self.send_image(image.key, &image.image_data).await?;
        }

        let mut buf = vec![0x00, 0x43, 0x52, 0x54, 0x00, 0x00, 0x53, 0x54, 0x50];
        self.write_extended_data(&mut buf).await?;

        cache.clear();

        Ok(())
    }

    /// Returns button state reader for this device
    pub fn get_reader(
        self: &Arc<Self>,
        process_input: fn(u8, u8) -> Result<DeviceInput, MirajazzError>,
    ) -> Arc<DeviceStateReader> {
        #[allow(clippy::arc_with_non_send_sync)]
        Arc::new(DeviceStateReader {
            device: self.clone(),
            states: Mutex::new(DeviceState {
                buttons: vec![false; self.key_count],
                encoders: vec![false; self.encoder_count],
            }),
            process_input,
        })
    }

    async fn write_image_data_reports(&self, image_data: &[u8]) -> Result<(), MirajazzError> {
        let image_report_length = self.packet_size + 1;
        let image_report_header_length = 1;
        let image_report_payload_length = image_report_length - image_report_header_length;

        let mut page_number = 0;
        let mut bytes_remaining = image_data.len();

        while bytes_remaining > 0 {
            let this_length = bytes_remaining.min(image_report_payload_length);
            let bytes_sent = page_number * image_report_payload_length;

            // Header
            let mut buf: Vec<u8> = [0x00].to_vec();
            buf.extend(&image_data[bytes_sent..bytes_sent + this_length]);

            // Adding padding
            buf.extend(vec![0u8; image_report_length - buf.len()]);

            self.write_data(&buf).await?;

            bytes_remaining -= this_length;
            page_number += 1;
        }

        Ok(())
    }

    /// Reads data from device
    pub async fn read_data(&self, length: usize) -> Result<Vec<u8>, MirajazzError> {
        let mut buf = vec![0u8; length];

        let _size = self
            .reader_writer
            .borrow_mut()
            .read_input_report(&mut buf)
            .await?;

        Ok(buf)
    }

    pub async fn read_data_with_timeout(
        &self,
        length: usize,
        timeout: Duration,
    ) -> Result<Option<Vec<u8>>, MirajazzError> {
        let mut buf = vec![0u8; length];

        let size = self
            .reader_writer
            .borrow_mut()
            .read_input_report(&mut buf)
            .or(async {
                Timer::after(timeout).await;
                Ok(0)
            })
            .await?;

        if size == 0 {
            return Ok(None);
        }

        Ok(Some(buf))
    }

    /// Writes data to device
    pub async fn write_data(&self, payload: &[u8]) -> Result<(), MirajazzError> {
        Ok(self
            .reader_writer
            .borrow_mut()
            .write_output_report(&payload)
            .await?)
    }

    /// Writes data to device extending payload to the required size
    pub async fn write_extended_data(&self, payload: &mut Vec<u8>) -> Result<(), MirajazzError> {
        payload.extend(vec![0u8; 1 + self.packet_size - payload.len()]);

        self.write_data(payload).await
    }
}
