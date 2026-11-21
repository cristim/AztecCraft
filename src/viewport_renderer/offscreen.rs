//! Offscreen colour readback shared by the regression harness and the playblast (#460).
//! wgpu only accepts texture-to-buffer copies whose row pitch is a multiple of 256 bytes, so
//! the mapped buffer is padded per row and the padding is stripped here.
use super::{RenderError, wgpu};
use std::{sync::mpsc, time::Duration};

const BYTES_PER_PIXEL: u32 = 4;
/// PROVISIONAL: how long one readback may wait for the GPU.
const READBACK_TIMEOUT: Duration = Duration::from_secs(10);

/// Row pitch of a `width`-pixel RGBA8 row after alignment; `None` when it overflows `u32`.
pub(super) fn padded_bytes_per_row(width: u32) -> Option<u32> {
    let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let row = width.checked_mul(BYTES_PER_PIXEL)?;
    Some(row.checked_add(align - 1)? / align * align)
}

/// Tightly packed rows of `width` pixels from a buffer whose rows are `padded` bytes apart.
pub(super) fn strip_row_padding(mapped: &[u8], width: u32, padded: u32) -> Vec<u8> {
    let row = (width * BYTES_PER_PIXEL) as usize;
    mapped
        .chunks_exact(padded as usize)
        .flat_map(|line| &line[..row])
        .copied()
        .collect()
}

/// Refuses a frame before any texture exists: an out-of-memory texture would latch the shared
/// device error that also disables the live viewport. The padded readback is the largest of the
/// colour, depth and readback allocations, so it stands for all three.
pub(super) fn check_size(
    limits: &wgpu::Limits,
    [width, height]: [u32; 2],
) -> Result<(), RenderError> {
    let edge = limits.max_texture_dimension_2d;
    if width == 0 || height == 0 || width > edge || height > edge {
        return Err(RenderError::ResourceLimit);
    }
    let bytes = padded_bytes_per_row(width).map(|padded| u64::from(padded) * u64::from(height));
    match bytes {
        Some(bytes) if bytes <= limits.max_buffer_size => Ok(()),
        _ => Err(RenderError::ResourceLimit),
    }
}

/// Copies an RGBA8 `texture` of `size` (width, height; it needs `COPY_SRC`) to the CPU as
/// `width * height * 4` tight bytes. Waits for earlier submissions on `queue` first.
pub(super) fn read_back(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
    size: [u32; 2],
) -> Result<Vec<u8>, RenderError> {
    let [width, height] = size;
    let limits = device.limits();
    check_size(&limits, size)?;
    let padded = padded_bytes_per_row(width).ok_or(RenderError::ResourceLimit)?;
    let bytes = u64::from(padded) * u64::from(height);
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("viewport readback pixels"),
        size: bytes,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &readback,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded),
                rows_per_image: Some(height),
            },
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
    let submitted = queue.submit([encoder.finish()]);
    let (send, receive) = mpsc::channel();
    readback
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |result| {
            let _ = send.send(result);
        });
    let backend = |what: &str, error: &dyn std::fmt::Display| {
        RenderError::Backend(format!("readback {what}: {error}"))
    };
    device
        .poll(wgpu::PollType::Wait {
            submission_index: Some(submitted),
            timeout: Some(READBACK_TIMEOUT),
        })
        .map_err(|error| backend("poll failed", &error))?;
    receive
        .recv_timeout(READBACK_TIMEOUT)
        .map_err(|error| backend("timed out", &error))?
        .map_err(|error| backend("map failed", &error))?;
    let pixels = {
        let mapped = readback
            .slice(..)
            .get_mapped_range()
            .map_err(|error| backend("range failed", &error))?;
        strip_row_padding(&mapped, width, padded)
    };
    readback.unmap();
    Ok(pixels)
}

#[cfg(test)]
#[path = "offscreen_tests.rs"]
mod tests;
