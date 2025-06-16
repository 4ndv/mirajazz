use async_hid::{Device as AsyncHidDevice, DeviceInfo as AsyncHidDeviceInfo};

pub type HidDeviceInfo = AsyncHidDeviceInfo;
pub type HidDevice = AsyncHidDevice;

/// Connection / Disconnection event for watchers
#[derive(Eq, PartialEq, Hash, Debug)]
pub enum DeviceLifecycleEvent {
    Connected(HidDeviceInfo),
    Disconnected(HidDeviceInfo),
}

/// Type of input that the device produced
#[derive(Clone, Debug)]
pub enum DeviceInput {
    /// No data was passed from the device
    NoData,

    /// Button was pressed
    ButtonStateChange(Vec<bool>),

    /// Encoder/Knob was pressed
    EncoderStateChange(Vec<bool>),

    /// Encoder/Knob was twisted/turned
    EncoderTwist(Vec<i8>),
}

impl DeviceInput {
    /// Checks if there's data received or not
    pub fn is_empty(&self) -> bool {
        matches!(self, DeviceInput::NoData)
    }
}

/// Image format used by the device
#[derive(Copy, Clone, Debug, Hash)]
pub struct ImageFormat {
    /// Image format/mode
    pub mode: ImageMode,
    /// Image size
    pub size: (usize, usize),
    /// Image rotation
    pub rotation: ImageRotation,
    /// Image mirroring
    pub mirror: ImageMirroring,
}

impl Default for ImageFormat {
    fn default() -> Self {
        Self {
            mode: ImageMode::None,
            size: (0, 0),
            rotation: ImageRotation::Rot0,
            mirror: ImageMirroring::None,
        }
    }
}

/// Image rotation
#[derive(Copy, Clone, Debug, Hash)]
pub enum ImageRotation {
    /// No rotation
    Rot0,
    /// 90 degrees clockwise
    Rot90,
    /// 180 degrees
    Rot180,
    /// 90 degrees counter-clockwise
    Rot270,
}

/// Image mirroring
#[derive(Copy, Clone, Debug, Hash)]
pub enum ImageMirroring {
    /// No image mirroring
    None,
    /// Flip by X
    X,
    /// Flip by Y
    Y,
    /// Flip by both axes
    Both,
}

/// Image format
#[derive(Copy, Clone, Debug, Hash)]
pub enum ImageMode {
    /// No image
    None,
    /// Bitmap image
    BMP,
    /// Jpeg image
    JPEG,
}
