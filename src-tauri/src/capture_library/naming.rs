//! Capture ids and file names: `Snipt_YYYYMMDD_HHMMSS[_NN].png` from the local capture time.

use chrono::{DateTime, Local};

use crate::shared::CaptureId;

pub const FILE_PREFIX: &str = "Snipt_";
const PNG_EXT: &str = ".png";
const ANNOTATED_SUFFIX: &str = "_annotated";
/// `_02` .. `_99`: at most 99 captures in one second.
const MAX_SEQ: u8 = 99;

/// `Snipt_<id>`: the file name of the original without the extension.
pub fn base_name(id: &CaptureId) -> String {
    format!("{FILE_PREFIX}{id}")
}

/// The first free id for `at`: `YYYYMMDD_HHMMSS`, then `_02` .. `_99`. `exists` gets the
/// base name and reports whether it is taken (in the index or on disk). `None` when all 99
/// are taken.
pub fn next_name(
    at: DateTime<Local>,
    exists: impl Fn(&str) -> bool,
) -> Option<(CaptureId, String)> {
    let stamp = at.format("%Y%m%d_%H%M%S").to_string();
    let candidates =
        std::iter::once(stamp.clone()).chain((2..=MAX_SEQ).map(|n| format!("{stamp}_{n:02}")));
    for candidate in candidates {
        let id = CaptureId::parse(&candidate).expect("formatted ids are valid");
        let base = base_name(&id);
        if !exists(&base) {
            return Some((id, base));
        }
    }
    None
}

/// What a file in the save folder is, judged by its name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SniptFile {
    Original(CaptureId),
    Annotated(CaptureId),
}

/// Recognizes `Snipt_<id>.png` and `Snipt_<id>_annotated.png`; anything else is `None`.
pub fn classify(file_name: &str) -> Option<SniptFile> {
    let stem = file_name.strip_prefix(FILE_PREFIX)?.strip_suffix(PNG_EXT)?;
    match stem.strip_suffix(ANNOTATED_SUFFIX) {
        Some(id) => CaptureId::parse(id).ok().map(SniptFile::Annotated),
        None => CaptureId::parse(stem).ok().map(SniptFile::Original),
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::collections::HashSet;

    use chrono::TimeZone;

    use super::*;

    fn at(h: u32, m: u32, s: u32) -> DateTime<Local> {
        Local.with_ymd_and_hms(2026, 10, 22, h, m, s).unwrap()
    }

    #[test]
    fn same_second_captures_get_sequence_numbers_in_order() {
        let taken = RefCell::new(HashSet::new());
        let mut ids = Vec::new();
        for _ in 0..3 {
            let (id, base) = next_name(at(17, 41, 50), |b| taken.borrow().contains(b)).unwrap();
            taken.borrow_mut().insert(base);
            ids.push(id);
        }
        let (later, _) = next_name(at(17, 41, 51), |b| taken.borrow().contains(b)).unwrap();
        ids.push(later);

        let names: Vec<&str> = ids.iter().map(CaptureId::as_str).collect();
        assert_eq!(
            names,
            [
                "20261022_174150",
                "20261022_174150_02",
                "20261022_174150_03",
                "20261022_174151",
            ]
        );
        let mut sorted = ids.clone();
        sorted.sort();
        assert_eq!(sorted, ids, "generation order equals lexical order");
    }

    #[test]
    fn base_names_carry_the_prefix() {
        let (id, base) = next_name(at(9, 5, 7), |_| false).unwrap();
        assert_eq!(id.as_str(), "20261022_090507");
        assert_eq!(base, "Snipt_20261022_090507");
        assert_eq!(base_name(&id), base);
    }

    #[test]
    fn gives_up_after_99_in_one_second() {
        assert_eq!(next_name(at(17, 41, 50), |_| true), None);
    }

    #[test]
    fn classifies_file_names() {
        let id = CaptureId::parse("20261022_174150_02").unwrap();
        assert_eq!(
            classify("Snipt_20261022_174150_02.png"),
            Some(SniptFile::Original(id.clone()))
        );
        assert_eq!(
            classify("Snipt_20261022_174150_02_annotated.png"),
            Some(SniptFile::Annotated(id))
        );
        for other in [
            "Snipt_20261022_174150.jpg",
            "Snipt_20261022_174150_01.png",
            "Other_20261022_174150.png",
            "Snipt_.png",
            "Snipt_20261022_174150_annotated_annotated.png",
            "index.json",
        ] {
            assert_eq!(classify(other), None, "{other}");
        }
    }
}
