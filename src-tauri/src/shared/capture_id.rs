use std::fmt;

use serde::{Deserialize, Serialize};

/// Identifies one capture: `"YYYYMMDD_HHMMSS"` from the local capture time, or
/// `"YYYYMMDD_HHMMSS_NN"` (NN = 02..99) for later captures in the same second.
///
/// Only digits and `_`, so it is safe in paths and in the `editor-<id>` window label, and the
/// lexical order is the capture order. Owned by capture-library.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct CaptureId(String);

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("invalid capture id: {0:?}")]
pub struct InvalidCaptureId(pub String);

const STAMP_LEN: usize = "YYYYMMDD_HHMMSS".len();

fn is_digits(s: &str) -> bool {
    s.bytes().all(|b| b.is_ascii_digit())
}

impl CaptureId {
    pub fn parse(s: &str) -> Result<Self, InvalidCaptureId> {
        let invalid = || InvalidCaptureId(s.to_owned());
        let (stamp, seq) = match s.len() {
            STAMP_LEN => (s, None),
            len if len == STAMP_LEN + 3 => (&s[..STAMP_LEN], Some(&s[STAMP_LEN..])),
            _ => return Err(invalid()),
        };
        if !s.is_ascii() {
            return Err(invalid());
        }
        let (date, time) = (&stamp[..8], &stamp[9..]);
        if &stamp[8..9] != "_" || !is_digits(date) || !is_digits(time) {
            return Err(invalid());
        }
        if let Some(seq) = seq {
            let number = seq.strip_prefix('_').filter(|n| is_digits(n));
            match number.and_then(|n| n.parse::<u8>().ok()) {
                Some(2..=99) => {}
                _ => return Err(invalid()),
            }
        }
        Ok(Self(s.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for CaptureId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl TryFrom<String> for CaptureId {
    type Error = InvalidCaptureId;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(&value)
    }
}

impl From<CaptureId> for String {
    fn from(id: CaptureId) -> Self {
        id.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_the_two_forms() {
        assert_eq!(
            CaptureId::parse("20261022_174150").unwrap().as_str(),
            "20261022_174150"
        );
        assert!(CaptureId::parse("20261022_174150_02").is_ok());
        assert!(CaptureId::parse("20261022_174150_99").is_ok());
    }

    #[test]
    fn rejects_anything_else() {
        for bad in [
            "",
            "..\\20261022_174150",
            "20261022_174150/",
            "20261022-174150",
            "2026102_174150",
            "20261022_1741500",
            "20261022_174150_01",
            "20261022_174150_00",
            "20261022_174150_100",
            "20261022_174150_2",
            "20261022_174150__2",
            "20261022_17415a",
            "２0261022_174150",
            "20261022_174150_+2",
        ] {
            assert!(CaptureId::parse(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn lexical_order_is_capture_order() {
        let mut ids: Vec<CaptureId> = [
            "20261022_174150_02",
            "20261023_000000",
            "20261022_174150",
            "20261022_174150_10",
            "20261022_174151",
            "20261022_174150_03",
        ]
        .iter()
        .map(|s| CaptureId::parse(s).unwrap())
        .collect();
        ids.sort();
        let sorted: Vec<&str> = ids.iter().map(CaptureId::as_str).collect();
        assert_eq!(
            sorted,
            [
                "20261022_174150",
                "20261022_174150_02",
                "20261022_174150_03",
                "20261022_174150_10",
                "20261022_174151",
                "20261023_000000",
            ]
        );
    }

    #[test]
    fn serializes_as_a_plain_string_and_validates_on_read() {
        let id = CaptureId::parse("20261022_174150_02").unwrap();
        assert_eq!(
            serde_json::to_string(&id).unwrap(),
            "\"20261022_174150_02\""
        );
        let read: CaptureId = serde_json::from_str("\"20261022_174150_02\"").unwrap();
        assert_eq!(read, id);
        assert!(serde_json::from_str::<CaptureId>("\"../etc\"").is_err());
    }
}
