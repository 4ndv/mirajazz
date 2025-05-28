use image::codecs::bmp::BmpEncoder;
use image::codecs::jpeg::JpegEncoder;
use image::imageops::FilterType;
use image::{ColorType, DynamicImage, GenericImageView, ImageError};

use crate::error::MirajazzError;
use crate::types::{ImageFormat, ImageMirroring, ImageMode, ImageRotation};

fn convert_image_with_format_impl(
    image_format: ImageFormat,
    image: DynamicImage,
) -> Result<Vec<u8>, ImageError> {
    // Ensuring size of the image
    let (ws, hs) = image_format.size;

    let image = image.resize_exact(ws as u32, hs as u32, FilterType::Nearest);

    // Applying rotation
    let image = match image_format.rotation {
        ImageRotation::Rot0 => image,
        ImageRotation::Rot90 => image.rotate90(),
        ImageRotation::Rot180 => image.rotate180(),
        ImageRotation::Rot270 => image.rotate270(),
    };

    // Applying mirroring
    let image = match image_format.mirror {
        ImageMirroring::None => image,
        ImageMirroring::X => image.fliph(),
        ImageMirroring::Y => image.flipv(),
        ImageMirroring::Both => image.fliph().flipv(),
    };

    let image_data = image.into_rgb8().to_vec();

    // Encoding image
    match image_format.mode {
        ImageMode::None => Ok(vec![]),
        ImageMode::BMP => {
            let mut buf = Vec::new();
            let mut encoder = BmpEncoder::new(&mut buf);
            encoder.encode(&image_data, ws as u32, hs as u32, ColorType::Rgb8.into())?;
            Ok(buf)
        }
        ImageMode::JPEG => {
            let mut buf = Vec::new();
            let mut encoder = JpegEncoder::new_with_quality(&mut buf, 90);
            encoder.encode(&image_data, ws as u32, hs as u32, ColorType::Rgb8.into())?;
            Ok(buf)
        }
    }
}

/// Converts image into image data depending on provided image format
pub async fn convert_image_with_format(
    image_format: ImageFormat,
    image: DynamicImage,
) -> Result<Vec<u8>, ImageError> {
    tokio::task::block_in_place(move || convert_image_with_format_impl(image_format, image))
}

/// Rect to be used when trying to send image to lcd screen
pub struct ImageRect {
    /// Width of the image
    pub w: u16,

    /// Height of the image
    pub h: u16,

    /// Data of the image row by row as RGB
    pub data: Vec<u8>,
}

impl ImageRect {
    /// Converts image to image rect
    pub fn from_image(image: DynamicImage) -> Result<ImageRect, MirajazzError> {
        let (image_w, image_h) = image.dimensions();

        let image_data = image.into_rgb8().to_vec();

        let mut buf = Vec::new();
        let mut encoder = JpegEncoder::new_with_quality(&mut buf, 90);
        encoder.encode(&image_data, image_w, image_h, ColorType::Rgb8.into())?;

        Ok(ImageRect {
            w: image_w as u16,
            h: image_h as u16,
            data: buf,
        })
    }
}
