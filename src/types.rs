/// Image format used by the Stream Deck
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
