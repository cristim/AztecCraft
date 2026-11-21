use super::*;
use crate::viewport_renderer::tests::device_setup;

#[test]
fn padded_pitch_is_the_next_multiple_of_256() {
    for (width, pitch) in [
        (1, 256),
        (63, 256),
        (64, 256),
        (65, 512),
        (100, 512),
        (130, 768),
    ] {
        assert_eq!(padded_bytes_per_row(width), Some(pitch), "width {width}");
    }
    assert_eq!(padded_bytes_per_row(u32::MAX), None);
    assert_eq!(padded_bytes_per_row(u32::MAX / 4), None);
}

#[test]
fn stripping_keeps_only_the_pixels_of_each_row() {
    let (width, padded) = (2, 8);
    let mapped: Vec<u8> = (0..24).collect();
    let rows: [&[u8]; 3] = [&mapped[0..8], &mapped[8..16], &mapped[16..24]];
    let expected: Vec<u8> = rows
        .iter()
        .flat_map(|row| row[..8].iter())
        .copied()
        .collect();
    assert_eq!(strip_row_padding(&mapped, width, padded), expected);
    // A pitch wider than the pixels drops the tail of every row.
    let wide: Vec<u8> = (0..24).collect();
    assert_eq!(
        strip_row_padding(&wide, 1, 8),
        vec![0, 1, 2, 3, 8, 9, 10, 11, 16, 17, 18, 19]
    );
}

fn pattern(width: u32, height: u32) -> Vec<u8> {
    (0..height)
        .flat_map(|y| (0..width).flat_map(move |x| [x as u8, y as u8, (x + 3 * y) as u8, 255]))
        .collect()
}

#[test]
#[ignore = "requires a real headless GPU adapter"]
fn headless_gpu_playblast_readback_strips_row_padding() {
    let (_, _, device, queue) = device_setup();
    // Neither square nor a multiple of 64 wide: 400, 520 bytes per row need padding.
    for (width, height) in [(100, 60), (130, 70), (1, 3), (63, 5), (64, 2)] {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("readback pattern"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let expected = pattern(width, height);
        queue.write_texture(
            texture.as_image_copy(),
            &expected,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(width * 4),
                rows_per_image: Some(height),
            },
            texture.size(),
        );
        let pixels = read_back(&device, &queue, &texture, [width, height]).unwrap();
        assert_eq!(pixels, expected, "{width}x{height}");
    }
}

#[test]
#[ignore = "requires a real headless GPU adapter"]
fn headless_gpu_playblast_readback_refuses_empty_and_oversize() {
    let (_, _, device, queue) = device_setup();
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("readback limits"),
        size: wgpu::Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let too_wide = device.limits().max_texture_dimension_2d + 1;
    for size in [[0, 1], [1, 0], [too_wide, 1]] {
        assert_eq!(
            read_back(&device, &queue, &texture, size),
            Err(RenderError::ResourceLimit)
        );
    }
}
