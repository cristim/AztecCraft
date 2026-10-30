use glam::Vec3;
use std::fs::File;
use std::io::{self, Write};
use std::path::Path;

pub struct Image {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<Vec3>,
}

impl Image {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            pixels: vec![Vec3::ZERO; (width * height) as usize],
        }
    }

    pub fn add_sample(&mut self, x: u32, y: u32, color: Vec3) {
        let index = (y * self.width + x) as usize;
        self.pixels[index] += color;
    }

    pub fn save_ppm(&self, path: &Path, samples: u32) -> io::Result<()> {
        let mut file = File::create(path)?;
        write!(file, "P6\n{} {}\n255\n", self.width, self.height)?;

        let mut buffer = Vec::with_capacity((self.width * self.height * 3) as usize);
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
