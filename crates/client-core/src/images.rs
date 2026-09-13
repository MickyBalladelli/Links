//! Shared image sizing policy.
//!
//! Native clients decode and re-encode pixels. This module keeps the maximum
//! edge and aspect-ratio calculation identical across those clients.

use thiserror::Error;

pub const MAX_IMAGE_EDGE: u32 = 1_600;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ImageError {
    #[error("image dimensions are invalid")]
    InvalidDimensions,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ImageDimensions {
    pub width: u32,
    pub height: u32,
}

/// Return dimensions after a proportional downscale. Never enlarges an image.
pub fn resized_dimensions(width: u32, height: u32) -> Result<ImageDimensions, ImageError> {
    if width == 0 || height == 0 {
        return Err(ImageError::InvalidDimensions);
    }
    if width <= MAX_IMAGE_EDGE && height <= MAX_IMAGE_EDGE {
        return Ok(ImageDimensions { width, height });
    }

    if width >= height {
        Ok(ImageDimensions {
            width: MAX_IMAGE_EDGE,
            height: scaled_edge(height, width),
        })
    } else {
        Ok(ImageDimensions {
            width: scaled_edge(width, height),
            height: MAX_IMAGE_EDGE,
        })
    }
}

fn scaled_edge(edge: u32, longest: u32) -> u32 {
    (((edge as u64 * MAX_IMAGE_EDGE as u64) + longest as u64 / 2)
        / longest as u64)
        .max(1) as u32
}
