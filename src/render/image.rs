use glam::Vec3;
use std::io::{self, Write};
use std::path::Path;

/// Largest accepted image side in pixels.
pub const MAX_DIMENSION: u32 = 8192;
/// Largest accepted pixel count (16 Mi pixels, 192 MiB of accumulation buffer).
pub const MAX_PIXELS: usize = 16 * 1024 * 1024;
/// Largest accepted samples-per-pixel count.
pub const MAX_SAMPLES: u32 = 4096;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderError {
    InvalidDimensions,
    ZeroSamples,
    InvalidDepth,
    InvalidCameraInput,
    ResourceLimit,
    DimensionsTooLarge,
    TooManySamples,
}

impl std::fmt::Display for RenderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidDimensions => f.write_str("width and height must be greater than 0"),
            Self::ZeroSamples => f.write_str("samples must be greater than 0"),
            Self::InvalidDepth => f.write_str("max depth must be between 1 and 16"),
            Self::InvalidCameraInput => {
                f.write_str("camera projection is invalid for this aspect ratio")
            }
            Self::ResourceLimit => f.write_str("image dimensions exceed resource limit"),
            Self::DimensionsTooLarge => f.write_str(
                "width and height must be at most 8192 and the pixel count at most 16777216",
            ),
            Self::TooManySamples => f.write_str("samples must be at most 4096"),
        }
    }
}
impl std::error::Error for RenderError {}

pub struct Image {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<Vec3>,
}

impl Image {
    pub fn new(width: u32, height: u32) -> Result<Self, RenderError> {
        if width == 0 || height == 0 {
            return Err(RenderError::InvalidDimensions);
        }
        if width > MAX_DIMENSION || height > MAX_DIMENSION {
            return Err(RenderError::DimensionsTooLarge);
        }
        let size = (width as usize)
            .checked_mul(height as usize)
            .ok_or(RenderError::ResourceLimit)?;
        if size > MAX_PIXELS {
            return Err(RenderError::DimensionsTooLarge);
        }

        let mut pixels = Vec::new();
        pixels
            .try_reserve_exact(size)
            .map_err(|_| RenderError::ResourceLimit)?;
        pixels.resize(size, Vec3::ZERO);

        Ok(Self {
            width,
            height,
            pixels,
        })
    }

    pub fn save_ppm(&self, path: &Path, samples: u32) -> io::Result<()> {
        if samples == 0 || samples > MAX_SAMPLES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "samples must be between 1 and 4096",
            ));
        }

        let expected_size = (self.width as usize)
            .checked_mul(self.height as usize)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "dimension overflow"))?;

        if self.pixels.len() != expected_size {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "pixel buffer size does not match width * height",
            ));
        }

        let mut buffer = Vec::with_capacity(15 + self.pixels.len() * 3);
        write!(&mut buffer, "P6\n{} {}\n255\n", self.width, self.height)?;

        let inv_samples = 1.0 / samples as f32;
        for pixel in &self.pixels {
            let color = pixel * inv_samples;
            let r = color.x.sqrt().clamp(0.0, 1.0);
            let g = color.y.sqrt().clamp(0.0, 1.0);
            let b = color.z.sqrt().clamp(0.0, 1.0);

            buffer.push((r * 255.0).round() as u8);
            buffer.push((g * 255.0).round() as u8);
            buffer.push((b * 255.0).round() as u8);
        }

        // write_new stages in a unique temp file and never replaces an existing destination.
        craft_atomic::write_new(path, &buffer)?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_rejects_oversized_dimensions() {
        assert_eq!(
            Image::new(MAX_DIMENSION + 1, 1).err(),
            Some(RenderError::DimensionsTooLarge)
        );
        assert_eq!(
            Image::new(u32::MAX, u32::MAX).err(),
            Some(RenderError::DimensionsTooLarge)
        );
        // Each side is allowed but the product exceeds MAX_PIXELS.
        assert_eq!(
            Image::new(MAX_DIMENSION, MAX_DIMENSION).err(),
            Some(RenderError::DimensionsTooLarge)
        );
        assert_eq!(Image::new(0, 4).err(), Some(RenderError::InvalidDimensions));
        assert!(Image::new(MAX_DIMENSION, 1).is_ok());
    }

    #[test]
    fn save_ppm_writes_expected_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("out.ppm");
        let mut image = Image::new(2, 1).unwrap();
        // Two samples accumulated: averages are 1.0 (white) and 0.25 (sqrt = 0.5).
        image.pixels[0] = Vec3::splat(2.0);
        image.pixels[1] = Vec3::splat(0.5);
        image.save_ppm(&path, 2).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        let mut expected = b"P6\n2 1\n255\n".to_vec();
        expected.extend_from_slice(&[255, 255, 255, 128, 128, 128]);
        assert_eq!(bytes, expected);
    }

    #[test]
    fn save_ppm_protects_occupied_destination_and_neighbours() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("scene.ppm");
        let neighbour = dir.path().join("scene.tmp");
        std::fs::write(&path, b"keep me").unwrap();
        std::fs::write(&neighbour, b"keep me too").unwrap();
        let image = Image::new(1, 1).unwrap();
        let error = image.save_ppm(&path, 1).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(std::fs::read(&path).unwrap(), b"keep me");
        assert_eq!(std::fs::read(&neighbour).unwrap(), b"keep me too");

        let fresh = dir.path().join("fresh.ppm");
        image.save_ppm(&fresh, 1).unwrap();
        assert_eq!(std::fs::read(&neighbour).unwrap(), b"keep me too");
    }

    #[test]
    fn save_ppm_rejects_bad_sample_counts() {
        let dir = tempfile::tempdir().unwrap();
        let image = Image::new(1, 1).unwrap();
        for samples in [0, MAX_SAMPLES + 1] {
            let path = dir.path().join(format!("s{samples}.ppm"));
            let error = image.save_ppm(&path, samples).unwrap_err();
            assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
            assert!(!path.exists());
        }
    }
}
