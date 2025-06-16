use async_hid::{
    AsyncHidWrite, Device as HidDevice, DeviceId, DeviceInfo as HidDeviceInfo, DeviceReader,
    DeviceWriter, HidBackend,
};
use futures_lite::{Stream, StreamExt};
use image::DynamicImage;
use std::{
    collections::{HashMap, HashSet},
    convert::identity,
    str::{from_utf8, Utf8Error},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};
use tokio::sync::Mutex;

use crate::{
    error::MirajazzError,
    images::convert_image_with_format,
    state::{DeviceState, DeviceStateReader},
    types::{DeviceInput, DeviceLifecycleEvent, ImageFormat},
};

/// Creates an instance of the async-hid backend
///
/// Can be used if you don't want to link async-hid crate into your project
pub fn new_hid_backend() -> HidBackend {
    HidBackend::default()
}

/// Struct for finding specific connected device
#[derive(Debug, Clone)]
pub struct DeviceQuery {
    usage_page: u16,
    usage_id: u16,
    vendor_id: u16,
    product_id: u16,
}

impl DeviceQuery {
    pub const fn new(usage_page: u16, usage_id: u16, vendor_id: u16, product_id: u16) -> Self {
        Self {
            usage_page,
            usage_id,
            vendor_id,
            product_id,
        }
    }
}

fn check_device(device: HidDevice, queries: &[DeviceQuery]) -> Option<HidDevice> {
    if !queries.iter().any(|query| {
        device.matches(
            query.usage_page,
            query.usage_id,
            query.vendor_id,
            query.product_id,
        )
    }) {
        return None;
    }

    if device.serial_number.is_some() {
        Some(device)
    } else {
        None
    }
}

/// Returns a list of devices as (Kind, Serial Number) that could be found using hid backend.
pub async fn list_devices(queries: &[DeviceQuery]) -> Result<HashSet<HidDevice>, MirajazzError> {
    let devices = HidBackend::default()
        .enumerate()
        .await?
        .filter_map(|d| check_device(d, queries))
        .collect::<HashSet<_>>()
        .await;

    Ok(devices)
}

pub struct DeviceWatcher {
    initialized: bool,
    id_map: Arc<Mutex<HashMap<DeviceId, HidDeviceInfo>>>,
    connected: Arc<Mutex<HashSet<HidDeviceInfo>>>,
}

impl DeviceWatcher {
    /// Builds new device watcher
    pub fn new() -> Self {
        Self {
            initialized: false,
            id_map: Arc::new(Mutex::new(HashMap::new())),
            connected: Arc::new(Mutex::new(HashSet::new())),
        }
    }

    /// Returns [Stream] of device connect/disconnect events
    ///
    /// **NOTE:** Only watches new events, to get already connected devices, use [list_devices]
    ///
    /// **NOTE:** Can only be called once per instance of [DeviceWatcher]
    pub async fn watch<'a>(
        &'a mut self,
        queries: &'a [DeviceQuery],
    ) -> Result<impl Stream<Item = DeviceLifecycleEvent> + Send + Unpin + use<'a>, MirajazzError>
    {
        let backend = HidBackend::default();

        if self.initialized {
            return Err(MirajazzError::WatcherAlreadyInitialized);
        }

        self.initialized = true;

        // We need to fill the devices list beforehand, because we need to track disconnected devices
        let already_connected = HidBackend::default()
            .enumerate()
            .await?
            .filter_map(|d| Some((d.id.clone(), check_device(d, queries)?)))
            .collect::<HashSet<_>>()
            .await;

        let mut map = self.id_map.lock().await;
        let mut connected = self.connected.lock().await;

        for (id, device) in already_connected.into_iter() {
            map.insert(id, device.clone());
            connected.insert(device.clone());
        }

        drop(map);
        drop(connected);

        let watcher = backend
            .watch()?
            .then(|e| async {
                match e {
                    async_hid::DeviceEvent::Connected(device_id) => {
                        let device = HidBackend::default()
                            .query_devices(&device_id)
                            .await
                            .unwrap()
                            .last()?;

                        let device = check_device(device, queries)?;
                        let info = device.clone();
                        drop(device);

                        self.id_map.lock().await.insert(device_id, info.clone());
                        let new = self.connected.lock().await.insert(info.clone());

                        if new {
                            Some(DeviceLifecycleEvent::Connected(info))
                        } else {
                            None
                        }
                    }
                    async_hid::DeviceEvent::Disconnected(device_id) => {
                        let info = self.id_map.lock().await.remove(&device_id)?;
                        let existed = self.connected.lock().await.remove(&info);

                        if existed {
                            Some(DeviceLifecycleEvent::Disconnected(info))
                        } else {
                            None
                        }
                    }
                }
            })
            .filter_map(identity);

        Ok(Box::pin(watcher))
    }
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
    /// Device reader
    reader: Arc<Mutex<DeviceReader>>,
    /// Device writer
    writer: Arc<Mutex<DeviceWriter>>,
    /// Temporarily cache the image before sending it to the device
    image_cache: Mutex<Vec<ImageCache>>,
    /// Device needs to be initialized
    initialized: AtomicBool,
}

/// Static functions of the struct
impl Device {
    /// Attempts to connect to the device
    pub async fn connect(
        dev: &HidDeviceInfo,
        is_v2: bool,
        supports_both_states: bool,
        key_count: usize,
        encoder_count: usize,
    ) -> Result<Device, MirajazzError> {
        let device = HidBackend::default().query_devices(&dev.id).await?.last();

        let device = match device {
            Some(device) => device,
            None => return Err(MirajazzError::DeviceNotFoundError),
        };

        let serial_number = match device.serial_number.clone() {
            Some(serial) => serial,
            None => return Err(MirajazzError::InvalidDeviceError),
        };

        let (reader, writer) = device.open().await?;

        Ok(Device {
            vid: device.vendor_id,
            pid: device.product_id,
            serial_number,
            is_v2,
            supports_both_states,
            key_count,
            encoder_count,
            reader: Arc::new(Mutex::new(reader)),
            writer: Arc::new(Mutex::new(writer)),
            packet_size: if is_v2 { 1024 } else { 512 },
            image_cache: Mutex::new(vec![]),
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

    /// Returns serial number of the device
    pub fn serial_number(&self) -> &String {
        &self.serial_number
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

    /// Returns value of `supports_both_states` field
    pub fn supports_both_states(&self) -> bool {
        self.supports_both_states
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

    /// Writes raw image data to the device, not to be used directly
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

    /// Writes image data to device, changes must be flushed with [Device::flush] before
    /// they will appear on the device!
    pub async fn write_image(&self, key: u8, image_data: &[u8]) -> Result<(), MirajazzError> {
        let cache_entry = ImageCache {
            key,
            image_data: image_data.to_vec(), // Convert &[u8] to Vec<u8>
        };

        self.image_cache.lock().await.push(cache_entry);

        Ok(())
    }

    /// Sets button's image to blank, changes must be flushed with [Device::flush] before
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

    /// Sets blank images to every button, changes must be flushed with [Device::flush] before
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

    /// Sets specified button's image, changes must be flushed with [Device::flush] before
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

    /// Puts device to sleep
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

    /// Flushes written images, updating displays
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
    ///
    /// Accepts function pointer for a function that maps raw device inputs to [DeviceInput]
    pub fn get_reader(
        &self,
        process_input: fn(u8, u8) -> Result<DeviceInput, MirajazzError>,
    ) -> Arc<DeviceStateReader> {
        #[allow(clippy::arc_with_non_send_sync)]
        Arc::new(DeviceStateReader {
            reader: self.reader.clone(),
            states: Mutex::new(DeviceState {
                buttons: vec![false; self.key_count],
                encoders: vec![false; self.encoder_count],
            }),
            supports_both_states: self.supports_both_states,
            process_input,
        })
    }

    /// Splits image data into chunks and writes them separately, not to be used directly
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

    /// Writes data to device
    pub async fn write_data(&self, payload: &[u8]) -> Result<(), MirajazzError> {
        Ok(self
            .writer
            .lock()
            .await
            .write_output_report(&payload)
            .await?)
    }

    /// Writes data to device extending payload to the required size
    pub async fn write_extended_data(&self, payload: &mut Vec<u8>) -> Result<(), MirajazzError> {
        payload.extend(vec![0u8; 1 + self.packet_size - payload.len()]);

        self.write_data(payload).await
    }
}
