//! The body of `editor_save`: `[u32 LE: JSON length][UTF-8 JSON][PNG]`. Only the JSON's
//! integer `schemaVersion` is checked here; the schema belongs to the WebView.

use super::error::EditorError;

pub const MAX_JSON_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_BODY_BYTES: usize = 512 * 1024 * 1024;
pub const PNG_SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];

#[derive(Debug, PartialEq)]
pub struct SaveEnvelope {
    pub annotations: serde_json::Value,
    pub png: Vec<u8>,
}

fn invalid(reason: &str) -> EditorError {
    EditorError::InvalidPayload(reason.to_owned())
}

pub fn decode(body: &[u8]) -> Result<SaveEnvelope, EditorError> {
    if body.len() > MAX_BODY_BYTES {
        return Err(invalid("データが大きすぎます"));
    }
    let (length, rest) = body
        .split_first_chunk::<4>()
        .ok_or_else(|| invalid("データが途中で切れています"))?;
    let length = u32::from_le_bytes(*length) as usize;
    if length > MAX_JSON_BYTES {
        return Err(invalid("注釈データが大きすぎます"));
    }
    if rest.len() < length {
        return Err(invalid("データが途中で切れています"));
    }
    let (json, png) = rest.split_at(length);
    let annotations: serde_json::Value =
        serde_json::from_slice(json).map_err(|_| invalid("注釈データが JSON ではありません"))?;
    let versioned = annotations
        .as_object()
        .and_then(|doc| doc.get("schemaVersion"))
        .is_some_and(|v| v.is_u64() || v.is_i64());
    if !versioned {
        return Err(invalid("注釈データに版番号がありません"));
    }
    if !png.starts_with(&PNG_SIGNATURE) {
        return Err(invalid("合成画像が PNG ではありません"));
    }
    Ok(SaveEnvelope {
        annotations,
        png: png.to_vec(),
    })
}

#[cfg(test)]
pub(crate) fn encode(json: &[u8], png: &[u8]) -> Vec<u8> {
    let mut body = (json.len() as u32).to_le_bytes().to_vec();
    body.extend_from_slice(json);
    body.extend_from_slice(png);
    body
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn png() -> Vec<u8> {
        let mut png = PNG_SIGNATURE.to_vec();
        png.extend_from_slice(b"rest");
        png
    }

    fn document() -> Vec<u8> {
        serde_json::to_vec(&json!({ "schemaVersion": 1, "shapes": [{ "text": "日本語" }] }))
            .unwrap()
    }

    fn reason(body: &[u8]) -> String {
        match decode(body) {
            Err(EditorError::InvalidPayload(reason)) => reason,
            other => panic!("expected invalidPayload, got {other:?}"),
        }
    }

    #[test]
    fn splits_the_json_and_the_png() {
        let envelope = decode(&encode(&document(), &png())).unwrap();
        assert_eq!(
            envelope.annotations,
            json!({ "schemaVersion": 1, "shapes": [{ "text": "日本語" }] })
        );
        assert_eq!(envelope.png, png());
    }

    #[test]
    fn rejects_truncated_bodies() {
        assert!(reason(&[]).contains("途中"));
        assert!(reason(&[1, 0]).contains("途中"));
        let body = encode(&document(), &png());
        assert!(reason(&body[..10]).contains("途中"));
    }

    #[test]
    fn rejects_an_oversized_json_length() {
        let mut body = ((MAX_JSON_BYTES + 1) as u32).to_le_bytes().to_vec();
        body.extend_from_slice(b"{}");
        assert!(reason(&body).contains("大きすぎます"));
    }

    #[test]
    fn needs_an_integer_schema_version() {
        for json in [
            &br#"{"shapes":[]}"#[..],
            br#"{"schemaVersion":"1"}"#,
            br#"{"schemaVersion":1.5}"#,
            br#"[1]"#,
            b"not json",
        ] {
            assert!(decode(&encode(json, &png())).is_err(), "{json:?}");
        }
        assert!(decode(&encode(br#"{"schemaVersion":2}"#, &png())).is_ok());
    }

    #[test]
    fn needs_a_png_signature() {
        assert!(reason(&encode(&document(), b"\xFF\xD8\xFF\xE0jpeg")).contains("PNG"));
        assert!(reason(&encode(&document(), &PNG_SIGNATURE[..7])).contains("PNG"));
    }
}
