use std::{
    error::Error,
    fmt::{Display, Formatter},
    str::Utf8Error,
    sync::PoisonError,
};

use async_hid::HidError;
use image::ImageError;

/// Errors that can occur while working with devices
#[derive(Debug)]
pub enum MirajazzError {
    /// Watcher is already initialized
    WatcherAlreadyInitialized,

    /// No device found for provided device info
    DeviceNotFoundError,

    /// Invalid device provided
    InvalidDeviceError,

    /// HidApi error
    HidError(HidError),

    /// Failed to convert bytes into string
    Utf8Error(Utf8Error),

    /// Failed to encode image
    ImageError(ImageError),

    /// Reader mutex was poisoned
    PoisonError,

    /// There's literally nowhere to write the image
    NoScreen,

    /// Key index is invalid
    InvalidKeyIndex,

    /// Unrecognized Product ID
    UnrecognizedPID,

    /// The device doesn't support doing that
    UnsupportedOperation,

    /// Device sent unexpected data
    BadData,
}

impl Display for MirajazzError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl Error for MirajazzError {}

impl From<HidError> for MirajazzError {
    fn from(e: HidError) -> Self {
        Self::HidError(e)
    }
}

impl From<Utf8Error> for MirajazzError {
    fn from(e: Utf8Error) -> Self {
        Self::Utf8Error(e)
    }
}

impl From<ImageError> for MirajazzError {
    fn from(e: ImageError) -> Self {
        Self::ImageError(e)
    }
}

impl<T> From<PoisonError<T>> for MirajazzError {
    fn from(_value: PoisonError<T>) -> Self {
        Self::PoisonError
    }
}
