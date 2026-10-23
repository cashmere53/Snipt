//! Pure conversions for the clipboard formats, kept apart from the Win32 calls so they can be
//! tested on every platform.

/// An image to put on the clipboard (`platform::windows::clipboard::copy_image`).
#[derive(Clone, Copy, Debug)]
pub struct ClipboardImage<'a> {
    pub width: u32,
    pub height: u32,
    /// Straight RGBA8, stride = width * 4.
    pub rgba: &'a [u8],
    /// The same image as PNG; `None` makes one with fast compression.
    pub png: Option<&'a [u8]>,
}

/// Size of `BITMAPV5HEADER`.
pub const BITMAPV5HEADER_SIZE: usize = 124;
const BI_BITFIELDS: u32 = 3;
/// `LCS_sRGB` (`'sRGB'`).
const LCS_SRGB: u32 = 0x7352_4742;
const LCS_GM_IMAGES: u32 = 4;

/// `CF_DIBV5` contents for straight RGBA8 pixels (stride = width * 4): a `BITMAPV5HEADER` with
/// alpha and BGRA bit fields, followed by the rows bottom-up as BGRA.
pub fn dibv5_from_rgba(width: u32, height: u32, rgba: &[u8]) -> Vec<u8> {
    let stride = width as usize * 4;
    assert_eq!(rgba.len(), stride * height as usize, "rgba length");
    let mut out = Vec::with_capacity(BITMAPV5HEADER_SIZE + rgba.len());
    for field in [
        BITMAPV5HEADER_SIZE as u32,
        width,
        height, // positive: bottom-up
    ] {
        out.extend_from_slice(&field.to_le_bytes());
    }
    out.extend_from_slice(&1u16.to_le_bytes()); // bV5Planes
    out.extend_from_slice(&32u16.to_le_bytes()); // bV5BitCount
    for field in [
        BI_BITFIELDS,
        rgba.len() as u32, // bV5SizeImage
        0,                 // bV5XPelsPerMeter
        0,                 // bV5YPelsPerMeter
        0,                 // bV5ClrUsed
        0,                 // bV5ClrImportant
        0x00FF_0000,       // red mask
        0x0000_FF00,       // green mask
        0x0000_00FF,       // blue mask
        0xFF00_0000,       // alpha mask
        LCS_SRGB,
    ] {
        out.extend_from_slice(&field.to_le_bytes());
    }
    out.extend_from_slice(&[0; 36]); // bV5Endpoints
    for field in [
        0, // bV5GammaRed
        0, // bV5GammaGreen
        0, // bV5GammaBlue
        LCS_GM_IMAGES,
        0, // bV5ProfileData
        0, // bV5ProfileSize
        0, // bV5Reserved
    ] {
        out.extend_from_slice(&u32::to_le_bytes(field));
    }
    debug_assert_eq!(out.len(), BITMAPV5HEADER_SIZE);

    for row in rgba.chunks_exact(stride.max(1)).rev() {
        for px in row.chunks_exact(4) {
            out.extend_from_slice(&[px[2], px[1], px[0], px[3]]);
        }
    }
    out
}

/// A PNG of straight RGBA8 pixels with fast compression.
pub fn png_from_rgba(width: u32, height: u32, rgba: &[u8]) -> Result<Vec<u8>, png::EncodingError> {
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_compression(png::Compression::Fast);
    let mut writer = encoder.write_header()?;
    writer.write_image_data(rgba)?;
    writer.finish()?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn u32_at(bytes: &[u8], offset: usize) -> u32 {
        u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
    }

    #[test]
    fn dibv5_header_describes_a_bottom_up_bgra_bitmap_with_alpha() {
        let dib = dibv5_from_rgba(2, 1, &[0; 8]);
        assert_eq!(dib.len(), BITMAPV5HEADER_SIZE + 8);
        assert_eq!(u32_at(&dib, 0), 124);
        assert_eq!(u32_at(&dib, 4), 2);
        assert_eq!(u32_at(&dib, 8), 1);
        assert_eq!(&dib[12..16], &[1, 0, 32, 0]);
        assert_eq!(u32_at(&dib, 16), BI_BITFIELDS);
        assert_eq!(u32_at(&dib, 20), 8);
        assert_eq!(u32_at(&dib, 40), 0x00FF_0000);
        assert_eq!(u32_at(&dib, 44), 0x0000_FF00);
        assert_eq!(u32_at(&dib, 48), 0x0000_00FF);
        assert_eq!(u32_at(&dib, 52), 0xFF00_0000);
        assert_eq!(u32_at(&dib, 56), LCS_SRGB);
        assert_eq!(u32_at(&dib, 108), LCS_GM_IMAGES);
    }

    #[test]
    fn dibv5_pixels_are_bgra_with_the_last_row_first() {
        // 1 x 2: top row red (half transparent), bottom row blue.
        let rgba = [255, 0, 0, 128, 0, 0, 255, 255];
        let dib = dibv5_from_rgba(1, 2, &rgba);
        let pixels = &dib[BITMAPV5HEADER_SIZE..];
        assert_eq!(pixels, &[255, 0, 0, 255, 0, 0, 255, 128]);
    }

    #[test]
    fn png_round_trips() {
        let rgba = [1, 2, 3, 4, 5, 6, 7, 8];
        let png = png_from_rgba(2, 1, &rgba).unwrap();
        let decoder = png::Decoder::new(std::io::Cursor::new(png));
        let mut reader = decoder.read_info().unwrap();
        let mut buf = vec![0; reader.output_buffer_size().unwrap()];
        let info = reader.next_frame(&mut buf).unwrap();
        assert_eq!((info.width, info.height), (2, 1));
        assert_eq!(&buf[..info.buffer_size()], &rgba);
    }
}
