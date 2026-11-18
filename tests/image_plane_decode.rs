//! Plate decoding limits and pixel values (#459). Synthetic fixtures only; expected values are
//! worked by hand, never read back from the decoder.

use std::io::Read;

use azteccraft::image_plane::decode::{
    MAX_PLATE_DIMENSION, MAX_PLATE_FILE_BYTES, MAX_PLATE_PIXELS, PROBE_BYTES, PlateError,
    PlateFormat, check_dimensions, decode_plate, probe_bytes, probe_reader,
};

fn png(
    width: u32,
    height: u32,
    color: png::ColorType,
    depth: png::BitDepth,
    data: &[u8],
) -> Vec<u8> {
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, width, height);
    encoder.set_color(color);
    encoder.set_depth(depth);
    encoder
        .write_header()
        .and_then(|mut writer| writer.write_image_data(data))
        .expect("encode synthetic png");
    out
}

fn rgba8(width: u32, height: u32, data: &[u8]) -> Vec<u8> {
    png(
        width,
        height,
        png::ColorType::Rgba,
        png::BitDepth::Eight,
        data,
    )
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = if crc & 1 == 1 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

/// Rewrites the IHDR size of an encoded PNG and recomputes the chunk CRC.
fn patch_size(mut file: Vec<u8>, width: u32, height: u32) -> Vec<u8> {
    file[16..20].copy_from_slice(&width.to_be_bytes());
    file[20..24].copy_from_slice(&height.to_be_bytes());
    let crc = crc32(&file[12..29]);
    file[29..33].copy_from_slice(&crc.to_be_bytes());
    file
}

/// A 1x1 file whose IHDR claims another size: the data behind the header cannot match it, so
/// only a check made before decoding can refuse it.
fn ihdr_only(width: u32, height: u32) -> Vec<u8> {
    patch_size(rgba8(1, 1, &[0, 0, 0, 0]), width, height)
}

fn chunk(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
    let mut out = (body.len() as u32).to_be_bytes().to_vec();
    let mut crc_input = kind.to_vec();
    crc_input.extend_from_slice(body);
    out.extend_from_slice(&crc_input);
    out.extend_from_slice(&crc32(&crc_input).to_be_bytes());
    out
}

#[test]
fn rgba_png_keeps_every_channel() {
    let data = [
        10, 20, 30, 40, 50, 60, 70, 80, 90, 100, 110, 120, 130, 140, 150, 160,
    ];
    let frame = decode_plate(&rgba8(2, 2, &data)).unwrap();
    assert_eq!((frame.width, frame.height), (2, 2));
    assert_eq!(frame.rgba, data);
}

#[test]
fn rgb_png_gets_opaque_alpha() {
    let data = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12];
    let frame = decode_plate(&png(2, 2, png::ColorType::Rgb, png::BitDepth::Eight, &data)).unwrap();
    assert_eq!(
        frame.rgba,
        [1, 2, 3, 255, 4, 5, 6, 255, 7, 8, 9, 255, 10, 11, 12, 255]
    );
}

#[test]
fn gray_and_gray_alpha_replicate_into_rgb() {
    let gray = png(
        3,
        1,
        png::ColorType::Grayscale,
        png::BitDepth::Eight,
        &[0, 128, 255],
    );
    assert_eq!(
        decode_plate(&gray).unwrap().rgba,
        [0, 0, 0, 255, 128, 128, 128, 255, 255, 255, 255, 255]
    );
    let gray_alpha = png(
        2,
        1,
        png::ColorType::GrayscaleAlpha,
        png::BitDepth::Eight,
        &[9, 7, 200, 1],
    );
    assert_eq!(
        decode_plate(&gray_alpha).unwrap().rgba,
        [9, 9, 9, 7, 200, 200, 200, 1]
    );
}

#[test]
fn palette_png_expands_through_the_palette_and_transparency() {
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, 3, 1);
    encoder.set_color(png::ColorType::Indexed);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_palette(vec![255, 0, 0, 0, 255, 0, 0, 0, 255]);
    encoder.set_trns(vec![100, 200]);
    let mut writer = encoder.write_header().unwrap();
    writer.write_image_data(&[2, 0, 1]).unwrap();
    writer.finish().unwrap();
    // Index 2 has no tRNS entry, so it is opaque; entries 0 and 1 carry 100 and 200.
    assert_eq!(
        decode_plate(&out).unwrap().rgba,
        [0, 0, 255, 255, 255, 0, 0, 100, 0, 255, 0, 200]
    );
}

#[test]
fn sixteen_bit_png_truncates_to_the_high_byte() {
    // 0x1234 gives 0x12, 0xABCD gives 0xAB; 0xFFFF gives 0xFF; alpha 0x0001 gives 0x00.
    let data = [0x12, 0x34, 0xAB, 0xCD, 0xFF, 0xFF, 0x00, 0x01];
    let file = png(1, 1, png::ColorType::Rgba, png::BitDepth::Sixteen, &data);
    assert_eq!(decode_plate(&file).unwrap().rgba, [0x12, 0xAB, 0xFF, 0x00]);
}

#[test]
fn interlaced_png_decodes_in_raster_order() {
    let file = std::fs::read("tests/fixtures/image_plane/interlaced_3x1.png").unwrap();
    assert_eq!(file[28], 1, "fixture must be Adam7 interlaced");
    assert_eq!(
        decode_plate(&file).unwrap().rgba,
        [255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255]
    );
}

#[test]
fn patched_huge_header_refuses_before_allocation() {
    let file = patch_size(rgba8(1, 1, &[1, 2, 3, 4]), 65535, 65535);
    assert_eq!(decode_plate(&file), Err(PlateError::TooLarge));
    assert_eq!(probe_bytes(&file), Err(PlateError::TooLarge));
}

#[test]
fn png_decoder_limits_alone_do_not_catch_the_oversize_header() {
    let file = ihdr_only(65535, 65535);
    let mut decoder = png::Decoder::new_with_limits(
        std::io::Cursor::new(file.as_slice()),
        png::Limits {
            bytes: usize::MAX / 2,
        },
    );
    decoder.set_transformations(png::Transformations::EXPAND);
    let reader = decoder
        .read_info()
        .expect("png accepts the oversize header");
    assert_eq!(reader.info().size(), (65535, 65535));
    assert_eq!(probe_bytes(&file), Err(PlateError::TooLarge));
}

#[test]
fn dimension_limit_boundaries() {
    assert_eq!(MAX_PLATE_DIMENSION, 8192);
    assert_eq!(MAX_PLATE_PIXELS, 33_554_432);
    assert_eq!(check_dimensions(8192, 1), Ok(8192 * 4));
    assert_eq!(check_dimensions(1, 8192), Ok(8192 * 4));
    assert_eq!(check_dimensions(8193, 1), Err(PlateError::TooLarge));
    assert_eq!(check_dimensions(1, 8193), Err(PlateError::TooLarge));
    assert_eq!(check_dimensions(8192, 4096), Ok(134_217_728));
    assert_eq!(check_dimensions(8192, 4097), Err(PlateError::TooLarge));
    assert_eq!(check_dimensions(0, 5), Err(PlateError::Corrupt));
    assert_eq!(check_dimensions(5, 0), Err(PlateError::Corrupt));
    assert_eq!(
        check_dimensions(u32::MAX, u32::MAX),
        Err(PlateError::TooLarge)
    );
}

#[test]
fn files_exactly_at_the_dimension_limit_decode_and_one_over_refuses() {
    let wide = rgba8(8192, 1, &vec![7u8; 8192 * 4]);
    let frame = decode_plate(&wide).unwrap();
    assert_eq!(
        (frame.width, frame.height, frame.rgba.len()),
        (8192, 1, 8192 * 4)
    );
    let tall = rgba8(1, 8192, &vec![7u8; 8192 * 4]);
    assert_eq!(decode_plate(&tall).unwrap().height, 8192);
    let over = rgba8(8193, 1, &vec![7u8; 8193 * 4]);
    assert_eq!(probe_bytes(&over), Err(PlateError::TooLarge));
    assert_eq!(decode_plate(&over), Err(PlateError::TooLarge));
}

#[test]
fn pixel_limit_refuses_from_a_header_with_no_data() {
    assert_eq!(
        probe_bytes(&ihdr_only(8192, 4096)).map(|i| (i.width, i.height)),
        Ok((8192, 4096))
    );
    assert_eq!(
        probe_bytes(&ihdr_only(8192, 4097)),
        Err(PlateError::TooLarge)
    );
    assert_eq!(
        decode_plate(&ihdr_only(8192, 4097)),
        Err(PlateError::TooLarge)
    );
}

#[test]
fn probe_reports_png_size_and_format() {
    let file = rgba8(3, 2, &[0; 24]);
    let info = probe_bytes(&file).unwrap();
    assert_eq!(
        (info.width, info.height, info.format),
        (3, 2, PlateFormat::Png)
    );
}

#[test]
fn header_behind_more_than_the_probe_window_is_header_too_large() {
    let base = rgba8(2, 2, &[0; 16]);
    let mut file = base[..33].to_vec();
    file.extend(chunk(b"prVt", &vec![0u8; PROBE_BYTES + 16]));
    file.extend_from_slice(&base[33..]);
    assert_eq!(
        probe_reader(file.as_slice()),
        Err(PlateError::HeaderTooLarge)
    );
    // The whole file is well inside the file limit and still decodes.
    assert_eq!(decode_plate(&file).unwrap().rgba, [0; 16]);
    // A short prefix that simply ends early is Truncated, not HeaderTooLarge.
    assert_eq!(probe_bytes(&base[..20]), Err(PlateError::Truncated));
}

struct Counting<'a> {
    inner: &'a [u8],
    total: usize,
}

impl Read for Counting<'_> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let read = self.inner.read(buf)?;
        self.total += read;
        Ok(read)
    }
}

#[test]
fn probe_never_reads_past_the_probe_window() {
    let mut file = rgba8(1, 1, &[0; 4]);
    file.extend(vec![0u8; 3 * PROBE_BYTES]);
    let mut reader = Counting {
        inner: &file,
        total: 0,
    };
    assert!(probe_reader(&mut reader).is_ok());
    assert!(reader.total <= PROBE_BYTES, "read {} bytes", reader.total);
}

#[test]
fn truncated_and_corrupt_data_never_yield_a_frame() {
    let file = rgba8(4, 4, &[9; 64]);
    assert_eq!(
        decode_plate(&file[..file.len() - 20]),
        Err(PlateError::Truncated)
    );
    // Flip a byte inside the IDAT body: the chunk CRC no longer matches.
    let mut bad = file.clone();
    let idat = bad.windows(4).position(|w| w == b"IDAT").unwrap() + 6;
    bad[idat] ^= 0xFF;
    assert_eq!(decode_plate(&bad), Err(PlateError::Corrupt));
    assert_eq!(
        decode_plate(b"\x89PNG\r\n\x1a\n"),
        Err(PlateError::Truncated)
    );
}

#[test]
fn exr_dpx_tiff_and_unknown_are_refused_by_name() {
    let refused = |bytes: &[u8], format| {
        assert_eq!(
            decode_plate(bytes),
            Err(PlateError::UnsupportedFormat { format })
        );
        assert_eq!(
            probe_bytes(bytes),
            Err(PlateError::UnsupportedFormat { format })
        );
    };
    refused(&[0x76, 0x2f, 0x31, 0x01, 2, 0, 0, 0, 0], PlateFormat::Exr);
    refused(b"SDPX\0\0\0\0\0", PlateFormat::Dpx);
    refused(b"XPDS\0\0\0\0\0", PlateFormat::Dpx);
    refused(b"II*\0\x08\0\0\0\0", PlateFormat::Tiff);
    refused(b"MM\0*\0\0\0\x08\0", PlateFormat::Tiff);
    refused(b"GIF89a-not-a-plate", PlateFormat::Unknown);
    assert_eq!(decode_plate(b""), Err(PlateError::Truncated));
    let message = PlateError::UnsupportedFormat {
        format: PlateFormat::Exr,
    }
    .to_string();
    assert!(message.contains("EXR"), "{message}");
}

#[test]
fn a_file_over_the_byte_limit_is_refused_before_parsing() {
    let mut oversize = rgba8(1, 1, &[0; 4]);
    oversize.resize(MAX_PLATE_FILE_BYTES + 1, 0);
    assert_eq!(decode_plate(&oversize), Err(PlateError::TooLarge));
}

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(format!("tests/fixtures/image_plane/{name}")).unwrap()
}

/// The 16x8 synthetic source of the colour fixtures: pixel (x, y) is (16x, 32y, 128).
fn within(
    frame: &azteccraft::image_plane::decode::PlateFrame,
    expected: impl Fn(u32, u32) -> [u8; 3],
) {
    const TOLERANCE: i32 = 6;
    for y in 0..8 {
        for x in 0..16 {
            let at = ((y * 16 + x) * 4) as usize;
            let want = expected(x, y);
            for (channel, &wanted) in want.iter().enumerate() {
                let got = i32::from(frame.rgba[at + channel]);
                assert!(
                    (got - i32::from(wanted)).abs() <= TOLERANCE,
                    "({x},{y}) channel {channel}: {got} vs {wanted}"
                );
            }
            assert_eq!(frame.rgba[at + 3], 255);
        }
    }
}

#[test]
fn baseline_and_progressive_jpegs_match_the_generating_gradient() {
    for name in ["gradient.jpg", "gradient_progressive.jpg"] {
        let frame = decode_plate(&fixture(name)).unwrap();
        assert_eq!((frame.width, frame.height), (16, 8), "{name}");
        within(&frame, |x, y| [(16 * x) as u8, (32 * y) as u8, 128]);
    }
}

#[test]
fn a_gray_jpeg_replicates_luma_into_rgb() {
    let frame = decode_plate(&fixture("gradient_gray.jpg")).unwrap();
    within(&frame, |x, y| {
        let v = (12 * x + 8 * y) as u8;
        [v, v, v]
    });
}

#[test]
fn jpeg_probe_reports_size_and_format() {
    let info = probe_bytes(&fixture("gradient.jpg")).unwrap();
    assert_eq!(
        (info.width, info.height, info.format),
        (16, 8, PlateFormat::Jpeg)
    );
}

#[test]
fn a_cmyk_jpeg_is_refused_by_name() {
    let refused = PlateError::UnsupportedFormat {
        format: PlateFormat::JpegCmyk,
    };
    assert_eq!(decode_plate(&fixture("cmyk.jpg")).unwrap_err(), refused);
    assert_eq!(probe_bytes(&fixture("cmyk.jpg")).unwrap_err(), refused);
}

#[test]
fn a_valid_9000_by_16_jpeg_is_refused_by_the_plate_limits_alone() {
    // 9000 is inside zune's own 16384 default, so only our dimension check can refuse it.
    let file = fixture("wide_9000x16.jpg");
    assert_eq!(probe_bytes(&file), Err(PlateError::TooLarge));
    assert_eq!(decode_plate(&file), Err(PlateError::TooLarge));
}

#[test]
fn a_huge_app_segment_before_the_frame_header_is_header_too_large() {
    let file = fixture("gradient.jpg");
    let mut padded = file[..2].to_vec();
    for _ in 0..17 {
        padded.extend_from_slice(&[0xFF, 0xE2, 0xFF, 0xFF]);
        padded.extend(vec![0u8; 0xFFFF - 2]);
    }
    padded.extend_from_slice(&file[2..]);
    assert!(padded.len() > PROBE_BYTES);
    assert_eq!(
        probe_reader(padded.as_slice()),
        Err(PlateError::HeaderTooLarge)
    );
    assert_eq!(decode_plate(&padded).unwrap().width, 16);
}

#[test]
fn truncated_and_corrupt_jpegs_never_yield_a_frame() {
    let file = fixture("gradient.jpg");
    assert!(matches!(
        decode_plate(&file[..file.len() - 40]),
        Err(PlateError::Truncated | PlateError::Corrupt)
    ));
    assert!(matches!(
        decode_plate(&file[..30]),
        Err(PlateError::Truncated | PlateError::Corrupt)
    ));
    assert_eq!(
        decode_plate(&[0xFF, 0xD8, 0xFF]),
        Err(PlateError::Truncated)
    );
}

/// Rewrites the size in the first SOF segment of a JPEG.
fn patch_jpeg_size(mut file: Vec<u8>, width: u16, height: u16) -> Vec<u8> {
    let at = file
        .windows(2)
        .position(|pair| matches!(pair, [0xFF, 0xC0 | 0xC2]))
        .unwrap();
    file[at + 5..at + 7].copy_from_slice(&height.to_be_bytes());
    file[at + 7..at + 9].copy_from_slice(&width.to_be_bytes());
    file
}

#[test]
fn progressive_jpegs_are_capped_at_4096_by_4096_pixels() {
    let progressive = fixture("gradient_progressive.jpg");
    let at_cap = patch_jpeg_size(progressive.clone(), 4096, 4096);
    let info = probe_bytes(&at_cap).unwrap();
    assert_eq!((info.width, info.height), (4096, 4096));
    for (width, height) in [(4097, 4096), (4096, 4097)] {
        let over = patch_jpeg_size(progressive.clone(), width, height);
        assert_eq!(
            probe_bytes(&over),
            Err(PlateError::TooLarge),
            "{width}x{height}"
        );
        assert_eq!(
            decode_plate(&over),
            Err(PlateError::TooLarge),
            "{width}x{height}"
        );
    }
}

#[test]
fn the_progressive_cap_does_not_apply_to_baseline_jpegs() {
    let baseline = patch_jpeg_size(fixture("gradient.jpg"), 8192, 4096);
    let info = probe_bytes(&baseline).unwrap();
    assert_eq!((info.width, info.height), (8192, 4096));
}

/// Inserts one junk byte before the first frame header, which zune's strict mode tolerates.
fn with_junk_before_sof(mut file: Vec<u8>) -> Vec<u8> {
    let at = file
        .windows(2)
        .position(|pair| matches!(pair, [0xFF, 0xC0 | 0xC2]))
        .unwrap();
    file.insert(at, 0x00);
    file
}

#[test]
fn a_junk_byte_before_the_frame_header_does_not_bypass_the_progressive_cap() {
    let junk = with_junk_before_sof(patch_jpeg_size(
        fixture("gradient_progressive.jpg"),
        8192,
        4096,
    ));
    assert_eq!(probe_bytes(&junk), Err(PlateError::TooLarge));
    assert_eq!(decode_plate(&junk), Err(PlateError::TooLarge));
}
