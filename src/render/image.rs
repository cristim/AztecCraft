use glam::Vec3;
use std::io::{self, Write};
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderError {
    InvalidDimensions,
    ZeroSamples,
    InvalidDepth,
    InvalidCameraInput,
    ResourceLimit,
    InvalidPixelCount,
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
            Self::InvalidPixelCount => f.write_str("pixel buffer size does not match dimensions"),
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
        let size = (width as usize)
            .checked_mul(height as usize)
            .ok_or(RenderError::ResourceLimit)?;

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

    pub fn add_sample(&mut self, x: u32, y: u32, color: Vec3) {
        if x >= self.width || y >= self.height {
            return; // Ignore out-of-bounds instead of panicking
        }
        let index = (y as usize) * (self.width as usize) + (x as usize);
        if let Some(pixel) = self.pixels.get_mut(index) {
            *pixel += color;
        }
    }

    pub fn save_ppm(&self, path: &Path, samples: u32) -> io::Result<()> {
        if samples == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "samples must be greater than 0",
            ));
        }

        // Validate that the pixel buffer matches the dimensions
        let expected_size = (self.width as usize) * (self.height as usize);
        if self.pixels.len() != expected_size {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "pixel buffer size does not match width * height",
            ));
        }

        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)?;

        write!(file, "P6\n{} {}\n255\n", self.width, self.height)?;

        let mut buffer = Vec::with_capacity(self.pixels.len() * 3);
        let inv_samples = 1.0 / samples as f32;

        for pixel in &self.pixels {
            let color = pixel * inv_samples;
            let r = color.x.sqrt().clamp(0.0, 1.0);
            let g = color.y.sqrt().clamp(0.0, 1.0);
            let b = color.z.sqrt().clamp(0.0, 1.0);

            buffer.push((r * 255.0) as u8);
            buffer.push((g * 255.0) as u8);
            buffer.push((b * 255.0) as u8);
        }

        file.write_all(&buffer)
    }
}
