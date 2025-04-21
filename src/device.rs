use std::{
    collections::HashSet,
    str::{from_utf8, Utf8Error},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, RwLock,
    },
    time::Duration,
};

use hidapi::{HidApi, HidDevice, HidResult};
use image::DynamicImage;

use crate::{
    error::MirajazzError,
    images::convert_image_with_format,
    state::{DeviceState, DeviceStateReader},
    types::{DeviceInput, ImageFormat},
};

/// Creates an instance of the HidApi
///
/// Can be used if you don't want to link hidapi crate into your project
pub fn new_hidapi() -> HidResult<HidApi> {
    HidApi::new()
}

/// Actually refreshes the device list
pub fn refresh_device_list(hidapi: &mut HidApi) -> HidResult<()> {
    hidapi.refresh_devices()
}

/// Returns a list of devices as (Kind, Serial Number) that could be found using HidApi.
///
/// **WARNING:** To refresh the list, use [refresh_device_list]
pub fn list_devices(hidapi: &HidApi, vids: &[u16]) -> Vec<(u16, u16, String)> {
    hidapi
        .device_list()
        .filter_map(|d| {
            if !vids.contains(&d.vendor_id()) {
                return None;
            }

            if let Some(serial) = d.serial_number() {
                Some((d.vendor_id(), d.product_id(), serial.to_string()))
            } else {
                None
            }
        })
        .collect::<HashSet<_>>()
        .into_iter()
        .collect()
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
    /// Use v2 hacks
    is_v2: bool,
    /// Number of keys
    key_count: usize,
    /// Number of encoders
    encoder_count: usize,
    /// Packet size
    packet_size: usize,
    /// Connected HIDDevice
    hid_device: HidDevice,
    /// Temporarily cache the image before sending it to the device
    image_cache: RwLock<Vec<ImageCache>>,
    /// Device needs to be initialized
    initialized: AtomicBool,
}

/// Static functions of the struct
impl Device {
    /// Attempts to connect to the device
    pub fn connect(
        hidapi: &HidApi,
        vid: u16,
        pid: u16,
        serial: &str,
        is_v2: bool,
        key_count: usize,
        encoder_count: usize,
    ) -> Result<Device, MirajazzError> {
        let hid_device = hidapi.open_serial(vid, pid, serial)?;

        Ok(Device {
            vid,
            pid,
            is_v2,
            key_count,
            encoder_count,
            packet_size: if is_v2 { 1024 } else { 512 },
            hid_device,
            image_cache: RwLock::new(vec![]),
            initialized: false.into(),
        })
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

    /// Returns manufacturer string of the device
    pub fn manufacturer(&self) -> Result<String, MirajazzError> {
        Ok(self
            .hid_device
            .get_manufacturer_string()?
            .unwrap_or_else(|| "Unknown".to_string()))
    }

    /// Returns product string of the device
    pub fn product(&self) -> Result<String, MirajazzError> {
        Ok(self
            .hid_device
            .get_product_string()?
            .unwrap_or_else(|| "Unknown".to_string()))
    }

    /// Returns serial number of the device
    pub fn serial_number(&self) -> Result<String, MirajazzError> {
        let serial = self.hid_device.get_serial_number_string()?;
        match serial {
            Some(serial) => {
                if serial.is_empty() {
                    Ok("Unknown".to_string())
                } else {
                    Ok(serial)
                }
            }
            None => Ok("Unknown".to_string()),
        }
        .map(|s| s.replace('\u{0001}', ""))
    }

    /// Returns firmware version of the device
    pub fn firmware_version(&self) -> Result<String, MirajazzError> {
        let bytes = self.get_feature_report(0x01, 20)?;

        Ok(extract_str(&bytes[0..])?)
    }

    /// Initializes the device
    fn initialize(&self) -> Result<(), MirajazzError> {
        if self.initialized.load(Ordering::Acquire) {
            return Ok(());
        }

        self.initialized.store(true, Ordering::Release);

        let mut buf = vec![0x00, 0x43, 0x52, 0x54, 0x00, 0x00, 0x44, 0x49, 0x53];
        self.write_extended_data(&mut buf)?;

        let mut buf = vec![
            0x00, 0x43, 0x52, 0x54, 0x00, 0x00, 0x4c, 0x49, 0x47, 0x00, 0x00, 0x00, 0x00,
        ];
        self.write_extended_data(&mut buf)?;

        Ok(())
    }

    /// Reads current input state from the device and calls provided function for processing
    pub fn read_input(
        &self,
        timeout: Option<Duration>,
        process_input: impl Fn(u8, u8) -> DeviceInput,
    ) -> Result<DeviceInput, MirajazzError> {
        self.initialize()?;

        let data = self.read_data(512, timeout)?;

        if data[0] == 0 {
            return Ok(DeviceInput::NoData);
        }

        Ok(process_input(data[9], data[10]))
    }

    /// Resets the device
    pub fn reset(&self) -> Result<(), MirajazzError> {
        self.initialize()?;

        self.set_brightness(100)?;
        self.clear_all_button_images()?;

        Ok(())
    }

    /// Sets brightness of the device, value range is 0 - 100
    pub fn set_brightness(&self, percent: u8) -> Result<(), MirajazzError> {
        self.initialize()?;

        let percent = percent.clamp(0, 100);

        let mut buf = vec![
            0x00, 0x43, 0x52, 0x54, 0x00, 0x00, 0x4c, 0x49, 0x47, 0x00, 0x00, percent,
        ];

        self.write_extended_data(&mut buf)?;

        Ok(())
    }

    fn send_image(&self, key: u8, image_data: &[u8]) -> Result<(), MirajazzError> {
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

        self.write_extended_data(&mut buf)?;

        self.write_image_data_reports(image_data)?;

        Ok(())
    }

    /// Writes image data to device, changes must be flushed with `.flush()` before
    /// they will appear on the device!
    pub fn write_image(&self, key: u8, image_data: &[u8]) -> Result<(), MirajazzError> {
        let cache_entry = ImageCache {
            key,
            image_data: image_data.to_vec(), // Convert &[u8] to Vec<u8>
        };

        self.image_cache.write()?.push(cache_entry);

        Ok(())
    }

    /// Sets button's image to blank, changes must be flushed with `.flush()` before
    /// they will appear on the device!
    pub fn clear_button_image(&self, key: u8) -> Result<(), MirajazzError> {
        self.initialize()?;

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

        self.write_extended_data(&mut buf)?;

        Ok(())
    }

    /// Sets blank images to every button, changes must be flushed with `.flush()` before
    /// they will appear on the device!
    pub fn clear_all_button_images(&self) -> Result<(), MirajazzError> {
        self.initialize()?;

        self.clear_button_image(0xFF)?;

        if self.is_v2 {
            // Mirabox "v2" requires STP to commit clearing the screen
            let mut buf = vec![0x00, 0x43, 0x52, 0x54, 0x00, 0x00, 0x53, 0x54, 0x50];

            self.write_extended_data(&mut buf)?;
        }

        Ok(())
    }

    /// Sets specified button's image, changes must be flushed with `.flush()` before
    /// they will appear on the device!
    pub fn set_button_image(
        &self,
        key: u8,
        image_format: ImageFormat,
        image: DynamicImage,
    ) -> Result<(), MirajazzError> {
        self.initialize()?;

        let image_data = convert_image_with_format(image_format, image)?;

        self.write_image(key, &image_data)?;

        Ok(())
    }

    /// Sleeps the device
    pub fn sleep(&self) -> Result<(), MirajazzError> {
        self.initialize()?;

        let mut buf = vec![0x00, 0x43, 0x52, 0x54, 0x00, 0x00, 0x48, 0x41, 0x4e];
        self.write_extended_data(&mut buf)?;

        Ok(())
    }

    /// Make periodic events to the device, to keep it alive
    pub fn keep_alive(&self) -> Result<(), MirajazzError> {
        self.initialize()?;

        let mut buf = vec![
            0x00, 0x43, 0x52, 0x54, 0x00, 0x00, 0x43, 0x4F, 0x4E, 0x4E, 0x45, 0x43, 0x54,
        ];

        self.write_extended_data(&mut buf)?;

        Ok(())
    }

    /// Shutdown the device
    pub fn shutdown(&self) -> Result<(), MirajazzError> {
        self.initialize()?;

        let mut buf = vec![
            0x00, 0x43, 0x52, 0x54, 0x00, 0x00, 0x43, 0x4c, 0x45, 0x00, 0x00, 0x44, 0x43,
        ];
        self.write_extended_data(&mut buf)?;

        let mut buf = vec![0x00, 0x43, 0x52, 0x54, 0x00, 0x00, 0x48, 0x41, 0x4E];
        self.write_extended_data(&mut buf)?;

        Ok(())
    }

    /// Flushes the button's image to the device
    pub fn flush(&self) -> Result<(), MirajazzError> {
        self.initialize()?;

        if self.image_cache.write()?.is_empty() {
            return Ok(());
        }

        for image in self.image_cache.read()?.iter() {
            self.send_image(image.key, &image.image_data)?;
        }

        let mut buf = vec![0x00, 0x43, 0x52, 0x54, 0x00, 0x00, 0x53, 0x54, 0x50];
        self.write_extended_data(&mut buf)?;

        self.image_cache.write()?.clear();

        Ok(())
    }

    /// Returns button state reader for this device
    pub fn get_reader(self: &Arc<Self>) -> Arc<DeviceStateReader> {
        #[allow(clippy::arc_with_non_send_sync)]
        Arc::new(DeviceStateReader {
            device: self.clone(),
            states: Mutex::new(DeviceState {
                buttons: vec![false; self.key_count],
                encoders: vec![false; self.encoder_count],
            }),
        })
    }

    fn write_image_data_reports(&self, image_data: &[u8]) -> Result<(), MirajazzError> {
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

            self.write_data(&buf)?;

            bytes_remaining -= this_length;
            page_number += 1;
        }

        Ok(())
    }

    /// Performs get_feature_report on [HidDevice]
    pub fn get_feature_report(
        &self,
        report_id: u8,
        length: usize,
    ) -> Result<Vec<u8>, MirajazzError> {
        let mut buff = vec![0u8; length];

        // Inserting report id byte
        buff.insert(0, report_id);

        // Getting feature report
        self.hid_device.get_feature_report(buff.as_mut_slice())?;

        Ok(buff)
    }

    /// Performs send_feature_report on [HidDevice]
    pub fn send_feature_report(&self, payload: &[u8]) -> Result<(), MirajazzError> {
        self.hid_device.send_feature_report(payload)?;

        Ok(())
    }

    /// Reads data from [HidDevice]. Blocking mode is used if timeout is specified
    pub fn read_data(
        &self,
        length: usize,
        timeout: Option<Duration>,
    ) -> Result<Vec<u8>, MirajazzError> {
        self.hid_device.set_blocking_mode(timeout.is_some())?;

        let mut buf = vec![0u8; length];

        match timeout {
            Some(timeout) => self
                .hid_device
                .read_timeout(buf.as_mut_slice(), timeout.as_millis() as i32),
            None => self.hid_device.read(buf.as_mut_slice()),
        }?;

        Ok(buf)
    }

    /// Writes data to [HidDevice]
    pub fn write_data(&self, payload: &[u8]) -> Result<usize, MirajazzError> {
        Ok(self.hid_device.write(payload)?)
    }

    /// Writes data to [HidDevice]
    pub fn write_extended_data(&self, payload: &mut Vec<u8>) -> Result<usize, MirajazzError> {
        payload.extend(vec![0u8; 1 + self.packet_size - payload.len()]);

        Ok(self.hid_device.write(payload)?)
    }
}
