//! Shared image sizing and placeholder policy.
//!
//! Native clients decode and re-encode pixels. This module keeps the maximum
//! edge, aspect-ratio, and BlurHash contract identical across those clients.

use thiserror::Error;

pub const MAX_IMAGE_EDGE: u32 = 1_600;
pub const BLUR_HASH_COMPONENTS_X: usize = 4;
pub const BLUR_HASH_COMPONENTS_Y: usize = 3;
pub const BLUR_HASH_SAMPLE_EDGE: usize = 32;
pub const BLUR_HASH_MAX_PIXELS: usize = (MAX_IMAGE_EDGE as usize) * (MAX_IMAGE_EDGE as usize);
pub const BLUR_HASH_LENGTH: usize = 28;

const BASE83: &[u8; 83] =
    b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz#$%*+,-.:;=?@[]^_{|}~";

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ImageError {
    #[error("image dimensions are invalid")]
    InvalidDimensions,
    #[error("image pixels are invalid")]
    InvalidPixels,
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

/// Encode an image as a fixed 4x3, 28-character BlurHash.
///
/// rgb contains row-major, 8-bit RGB pixels. The source is box-sampled to a
/// maximum 32x32 grid before the DCT, so large images stay cheap to process.
/// The returned hash is suitable for MediaMetadata.blur_hash, which remains
/// inside the encrypted Message and never crosses the upload boundary.
pub fn encode_blur_hash(width: u32, height: u32, rgb: &[u8]) -> Result<String, ImageError> {
    let width = usize::try_from(width).map_err(|_| ImageError::InvalidDimensions)?;
    let height = usize::try_from(height).map_err(|_| ImageError::InvalidDimensions)?;
    if width == 0 || height == 0 {
        return Err(ImageError::InvalidDimensions);
    }
    let pixels = width
        .checked_mul(height)
        .ok_or(ImageError::InvalidDimensions)?;
    let expected_bytes = pixels.checked_mul(3).ok_or(ImageError::InvalidPixels)?;
    if pixels > BLUR_HASH_MAX_PIXELS || rgb.len() != expected_bytes {
        return Err(ImageError::InvalidPixels);
    }

    let (sample_width, sample_height, samples) = sample_pixels(width, height, rgb);
    let mut components = Vec::with_capacity(BLUR_HASH_COMPONENTS_X * BLUR_HASH_COMPONENTS_Y);
    for component_y in 0..BLUR_HASH_COMPONENTS_Y {
        for component_x in 0..BLUR_HASH_COMPONENTS_X {
            components.push(dct_component(
                sample_width,
                sample_height,
                &samples,
                component_x,
                component_y,
            ));
        }
    }

    let mut hash = String::with_capacity(BLUR_HASH_LENGTH);
    let size_value =
        (BLUR_HASH_COMPONENTS_X - 1) as u32 + ((BLUR_HASH_COMPONENTS_Y - 1) as u32 * 9);
    append_base83(&mut hash, size_value, 1);

    let maximum_ac = components
        .iter()
        .skip(1)
        .flat_map(|component| component.iter())
        .map(|value| value.abs())
        .fold(0.0_f32, f32::max);
    let maximum_ac_quantised = ((maximum_ac * 166.0) - 0.5).floor().clamp(0.0, 255.0) as u32;
    let maximum_ac_value = (maximum_ac_quantised as f32 + 1.0) / 166.0;
    append_base83(&mut hash, maximum_ac_quantised, 1);

    append_base83(&mut hash, encode_dc(components[0]), 4);
    for component in components.iter().skip(1) {
        let red = encode_ac(component[0], maximum_ac_value);
        let green = encode_ac(component[1], maximum_ac_value);
        let blue = encode_ac(component[2], maximum_ac_value);
        append_base83(&mut hash, red * 19 * 19 + green * 19 + blue, 2);
    }
    debug_assert_eq!(hash.len(), BLUR_HASH_LENGTH);
    Ok(hash)
}

fn sample_pixels(width: usize, height: usize, rgb: &[u8]) -> (usize, usize, Vec<[f32; 3]>) {
    let sample_width = width.min(BLUR_HASH_SAMPLE_EDGE);
    let sample_height = height.min(BLUR_HASH_SAMPLE_EDGE);
    let mut samples = Vec::with_capacity(sample_width * sample_height);
    for sample_y in 0..sample_height {
        let start_y = sample_y * height / sample_height;
        let end_y = ((sample_y + 1) * height / sample_height).max(start_y + 1);
        for sample_x in 0..sample_width {
            let start_x = sample_x * width / sample_width;
            let end_x = ((sample_x + 1) * width / sample_width).max(start_x + 1);
            let mut total = [0.0_f32; 3];
            let mut count = 0.0_f32;
            for y in start_y..end_y.min(height) {
                for x in start_x..end_x.min(width) {
                    let offset = (y * width + x) * 3;
                    total[0] += srgb_to_linear(rgb[offset]);
                    total[1] += srgb_to_linear(rgb[offset + 1]);
                    total[2] += srgb_to_linear(rgb[offset + 2]);
                    count += 1.0;
                }
            }
            samples.push([total[0] / count, total[1] / count, total[2] / count]);
        }
    }
    (sample_width, sample_height, samples)
}

fn dct_component(
    width: usize,
    height: usize,
    samples: &[[f32; 3]],
    component_x: usize,
    component_y: usize,
) -> [f32; 3] {
    let mut value = [0.0_f32; 3];
    let normalisation = if component_x == 0 && component_y == 0 {
        1.0
    } else {
        2.0
    };
    let scale = normalisation / (width * height) as f32;
    for y in 0..height {
        for x in 0..width {
            let basis = (std::f32::consts::PI * component_x as f32 * x as f32 / width as f32).cos()
                * (std::f32::consts::PI * component_y as f32 * y as f32 / height as f32).cos();
            let sample = samples[y * width + x];
            value[0] += sample[0] * basis;
            value[1] += sample[1] * basis;
            value[2] += sample[2] * basis;
        }
    }
    [value[0] * scale, value[1] * scale, value[2] * scale]
}

fn encode_dc(value: [f32; 3]) -> u32 {
    (srgb_to_byte(value[0]) << 16) | (srgb_to_byte(value[1]) << 8) | srgb_to_byte(value[2])
}

fn encode_ac(value: f32, maximum: f32) -> u32 {
    (sign_pow(value / maximum, 0.5) * 9.0 + 9.5)
        .floor()
        .clamp(0.0, 18.0) as u32
}

fn sign_pow(value: f32, exponent: f32) -> f32 {
    value.signum() * value.abs().powf(exponent)
}

fn srgb_to_linear(value: u8) -> f32 {
    let value = value as f32 / 255.0;
    if value <= 0.04045 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}

fn srgb_to_byte(value: f32) -> u32 {
    let value = if value <= 0.0031308 {
        value * 12.92
    } else {
        1.055 * value.powf(1.0 / 2.4) - 0.055
    };
    (value * 255.0).round().clamp(0.0, 255.0) as u32
}

fn append_base83(output: &mut String, mut value: u32, length: usize) {
    let mut divisor = 1_u32;
    for _ in 1..length {
        divisor *= 83;
    }
    for _ in 0..length {
        let digit = (value / divisor) % 83;
        output.push(BASE83[digit as usize] as char);
        value %= divisor;
        if divisor > 1 {
            divisor /= 83;
        }
    }
}

fn scaled_edge(edge: u32, longest: u32) -> u32 {
    (((edge as u64 * MAX_IMAGE_EDGE as u64) + longest as u64 / 2) / longest as u64).max(1) as u32
}
