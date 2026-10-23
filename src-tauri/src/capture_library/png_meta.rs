//! Original PNGs with Snipt metadata in an iTXt chunk `snipt:meta`, placed right after IHDR so
//! reading it only touches the first chunks.

use std::io::{BufRead, Cursor, Seek};

use chrono::{DateTime, FixedOffset};
use serde::{Deserialize, Serialize};

use super::types::{CaptureImage, LibraryError};
use crate::platform::image_formats::png_from_rgba;
use crate::shared::{CaptureId, CaptureMode};

pub const META_KEYWORD: &str = "snipt:meta";
const SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1A, b'\n'];

/// `{ "v": 1, "id", "capturedAt", "mode", "windowTitle" }`. Readers ignore unknown fields.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SniptMetaV1 {
    pub v: u32,
    pub id: CaptureId,
    pub captured_at: DateTime<FixedOffset>,
    pub mode: CaptureMode,
    #[serde(default)]
    pub window_title: Option<String>,
}

fn invalid(message: impl Into<String>) -> LibraryError {
    LibraryError::InvalidImage {
        message: message.into(),
    }
}

/// The PNG to write as the original: RGBA is encoded with fast compression, a PNG gets its
/// metadata replaced. Rejects empty images, wrong pixel lengths and broken PNGs.
pub fn encode(image: &CaptureImage, meta: &SniptMetaV1) -> Result<Vec<u8>, LibraryError> {
    let png = match image {
        CaptureImage::Rgba {
            width,
            height,
            pixels,
        } => {
            let expected = *width as u64 * *height as u64 * 4;
            if expected == 0 {
                return Err(invalid("画像の大きさが 0 です"));
            }
            if pixels.len() as u64 != expected {
                return Err(invalid(format!(
                    "画素データの長さが {} バイトではなく {} バイトです",
                    expected,
                    pixels.len()
                )));
            }
            png_from_rgba(*width, *height, pixels).map_err(|e| invalid(e.to_string()))?
        }
        CaptureImage::Png(bytes) => {
            let (width, height, _) = read_header_from(Cursor::new(bytes))?;
            if width == 0 || height == 0 {
                return Err(invalid("画像の大きさが 0 です"));
            }
            bytes.clone()
        }
    };
    let json = serde_json::to_string(meta).expect("metadata always serializes");
    with_meta_chunk(&png, &json)
}

/// Width, height and Snipt metadata (if present and readable) from the first chunks.
pub fn read_header_from(
    reader: impl BufRead + Seek,
) -> Result<(u32, u32, Option<SniptMetaV1>), LibraryError> {
    let reader = png::Decoder::new(reader)
        .read_info()
        .map_err(|e| invalid(e.to_string()))?;
    let info = reader.info();
    let meta = info
        .utf8_text
        .iter()
        .find(|chunk| chunk.keyword == META_KEYWORD)
        .and_then(|chunk| chunk.get_text().ok())
        .and_then(|text| serde_json::from_str::<SniptMetaV1>(&text).ok());
    Ok((info.width, info.height, meta))
}

/// `read_header_from` for a file.
pub fn read_header(
    path: &std::path::Path,
) -> Result<(u32, u32, Option<SniptMetaV1>), LibraryError> {
    let file = std::fs::File::open(path).map_err(|e| super::atomic_fs::fs_error(path, &e))?;
    read_header_from(std::io::BufReader::new(file))
}

/// Rewrites the chunk list of `png`: drops any existing `snipt:meta` iTXt chunk and inserts a
/// new uncompressed one after IHDR. Chunk CRCs are checked, so a corrupt file is rejected.
fn with_meta_chunk(png: &[u8], json: &str) -> Result<Vec<u8>, LibraryError> {
    let body = png
        .strip_prefix(&SIGNATURE)
        .ok_or_else(|| invalid("PNG の署名がありません"))?;
    let mut out = Vec::with_capacity(png.len() + json.len() + 64);
    out.extend_from_slice(&SIGNATURE);
    let mut rest = body;
    let mut first = true;
    let mut ended = false;
    while !rest.is_empty() {
        let (kind, data, total) = split_chunk(rest)?;
        if first && kind != *b"IHDR" {
            return Err(invalid("PNG の先頭が IHDR ではありません"));
        }
        if !is_meta_chunk(&kind, data) {
            out.extend_from_slice(&rest[..total]);
        }
        if first {
            write_chunk(&mut out, b"iTXt", &itxt_data(json));
            first = false;
        }
        rest = &rest[total..];
        if kind == *b"IEND" {
            ended = true;
            break;
        }
    }
    if !ended {
        return Err(invalid("PNG が途中で切れています"));
    }
    Ok(out)
}

/// Returns (type, data, whole chunk length) for the chunk at the start of `bytes`.
fn split_chunk(bytes: &[u8]) -> Result<([u8; 4], &[u8], usize), LibraryError> {
    let truncated = || invalid("PNG が途中で切れています");
    let length =
        u32::from_be_bytes(bytes.get(..4).ok_or_else(truncated)?.try_into().unwrap()) as usize;
    let total = length.checked_add(12).ok_or_else(truncated)?;
    let chunk = bytes.get(..total).ok_or_else(truncated)?;
    let kind: [u8; 4] = chunk[4..8].try_into().unwrap();
    let data = &chunk[8..8 + length];
    let crc = u32::from_be_bytes(chunk[8 + length..].try_into().unwrap());
    if crc32fast::hash(&chunk[4..8 + length]) != crc {
        return Err(invalid("PNG のチャンクが壊れています"));
    }
    Ok((kind, data, total))
}

fn is_meta_chunk(kind: &[u8; 4], data: &[u8]) -> bool {
    kind == b"iTXt"
        && data.starts_with(META_KEYWORD.as_bytes())
        && data.get(META_KEYWORD.len()) == Some(&0)
}

/// iTXt data: keyword NUL, compression flag 0, method 0, empty language NUL, empty translated
/// keyword NUL, UTF-8 text.
fn itxt_data(text: &str) -> Vec<u8> {
    let mut data = Vec::with_capacity(META_KEYWORD.len() + 5 + text.len());
    data.extend_from_slice(META_KEYWORD.as_bytes());
    data.extend_from_slice(&[0, 0, 0, 0, 0]);
    data.extend_from_slice(text.as_bytes());
    data
}

fn write_chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    let start = out.len();
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    let crc = crc32fast::hash(&out[start..]);
    out.extend_from_slice(&crc.to_be_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meta(title: Option<&str>) -> SniptMetaV1 {
        SniptMetaV1 {
            v: 1,
            id: CaptureId::parse("20261022_174150").unwrap(),
            captured_at: DateTime::parse_from_rfc3339("2026-10-22T17:41:50.123+09:00").unwrap(),
            mode: CaptureMode::Window,
            window_title: title.map(str::to_owned),
        }
    }

    fn rgba(width: u32, height: u32) -> CaptureImage {
        CaptureImage::Rgba {
            width,
            height,
            pixels: (0..width * height * 4).map(|i| i as u8).collect(),
        }
    }

    fn decode_pixels(png: &[u8]) -> Vec<u8> {
        let mut reader = png::Decoder::new(Cursor::new(png)).read_info().unwrap();
        let mut buf = vec![0; reader.output_buffer_size().unwrap()];
        let info = reader.next_frame(&mut buf).unwrap();
        buf.truncate(info.buffer_size());
        buf
    }

    #[test]
    fn metadata_round_trips_with_a_japanese_window_title() {
        let png = encode(&rgba(3, 2), &meta(Some("無題 - メモ帳"))).unwrap();
        let (width, height, read) = read_header_from(Cursor::new(&png)).unwrap();
        assert_eq!((width, height), (3, 2));
        assert_eq!(read, Some(meta(Some("無題 - メモ帳"))));
        let CaptureImage::Rgba { pixels, .. } = rgba(3, 2) else {
            unreachable!()
        };
        assert_eq!(decode_pixels(&png), pixels);
    }

    #[test]
    fn png_input_gets_its_metadata_replaced() {
        let first = encode(&rgba(2, 2), &meta(Some("old"))).unwrap();
        let second = encode(&CaptureImage::Png(first.clone()), &meta(None)).unwrap();

        let (_, _, read) = read_header_from(Cursor::new(&second)).unwrap();
        assert_eq!(read, Some(meta(None)));
        assert_eq!(decode_pixels(&second), decode_pixels(&first));
        let count = second.windows(4).filter(|w| *w == b"iTXt").count();
        assert_eq!(count, 1, "the old chunk is dropped");
    }

    #[test]
    fn plain_pngs_have_no_metadata() {
        let png = png_from_rgba(1, 1, &[0, 0, 0, 255]).unwrap();
        assert_eq!(read_header_from(Cursor::new(&png)).unwrap(), (1, 1, None));
    }

    #[test]
    fn unknown_fields_and_versions_are_tolerated() {
        let json = r#"{"v":7,"id":"20261022_174150","capturedAt":"2026-10-22T17:41:50+09:00","mode":"region","extra":true}"#;
        let png = png_from_rgba(1, 1, &[0, 0, 0, 255]).unwrap();
        let tagged = with_meta_chunk(&png, json).unwrap();
        let (_, _, read) = read_header_from(Cursor::new(&tagged)).unwrap();
        let read = read.unwrap();
        assert_eq!(read.v, 7);
        assert_eq!(read.mode, CaptureMode::Region);
        assert_eq!(read.window_title, None);
    }

    #[test]
    fn rejects_invalid_input() {
        assert_eq!(
            encode(&rgba(0, 5), &meta(None)).unwrap_err().kind(),
            "invalidImage"
        );
        let short = CaptureImage::Rgba {
            width: 2,
            height: 2,
            pixels: vec![0; 15],
        };
        assert_eq!(
            encode(&short, &meta(None)).unwrap_err().kind(),
            "invalidImage"
        );
        assert_eq!(
            encode(&CaptureImage::Png(b"not a png".to_vec()), &meta(None))
                .unwrap_err()
                .kind(),
            "invalidImage"
        );

        let mut corrupt = png_from_rgba(1, 1, &[0, 0, 0, 255]).unwrap();
        let last = corrupt.len() - 20;
        corrupt[last] ^= 0xFF;
        assert_eq!(
            encode(&CaptureImage::Png(corrupt), &meta(None))
                .unwrap_err()
                .kind(),
            "invalidImage"
        );

        let full = png_from_rgba(1, 1, &[0, 0, 0, 255]).unwrap();
        let truncated = full[..full.len() - 12].to_vec();
        assert_eq!(
            encode(&CaptureImage::Png(truncated), &meta(None))
                .unwrap_err()
                .kind(),
            "invalidImage"
        );
    }

    #[test]
    fn reads_the_header_from_a_file() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("a.png");
        std::fs::write(&path, encode(&rgba(4, 3), &meta(None)).unwrap()).unwrap();
        let (width, height, read) = read_header(&path).unwrap();
        assert_eq!((width, height), (4, 3));
        assert_eq!(read.unwrap().id.as_str(), "20261022_174150");
        assert_eq!(
            read_header(&root.path().join("missing.png"))
                .unwrap_err()
                .kind(),
            "io"
        );
    }
}
