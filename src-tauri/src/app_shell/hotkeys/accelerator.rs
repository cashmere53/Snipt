//! Normalizes, validates, converts and displays accelerators.
//!
//! Format: `Ctrl+Alt+Shift+Win+<code>` where `<code>` is a W3C `KeyboardEvent.code` name.
//! Keys other than PrintScreen, F1-F24, Pause and ScrollLock need at least one modifier so
//! that ordinary typing never triggers a capture.

use std::str::FromStr;

use serde::Serialize;
use tauri_plugin_global_shortcut::{Code, Modifiers, Shortcut};

use crate::shared::Accelerator;

/// Why an accelerator was rejected. Serialized as `"noKey" | "modifierRequired" | "unknownKey"`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum InvalidAccelerator {
    /// Empty, or only modifiers.
    NoKey,
    /// A key that is only allowed together with a modifier.
    ModifierRequired,
    /// An unknown token, or more than one non-modifier key.
    UnknownKey,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Modifier {
    Ctrl,
    Alt,
    Shift,
    Win,
}

impl Modifier {
    /// In normalized order.
    const ALL: [Modifier; 4] = [
        Modifier::Ctrl,
        Modifier::Alt,
        Modifier::Shift,
        Modifier::Win,
    ];

    fn parse(token: &str) -> Option<Self> {
        match token.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => Some(Self::Ctrl),
            "alt" => Some(Self::Alt),
            "shift" => Some(Self::Shift),
            "win" | "super" | "meta" | "cmd" => Some(Self::Win),
            _ => None,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Ctrl => "Ctrl",
            Self::Alt => "Alt",
            Self::Shift => "Shift",
            Self::Win => "Win",
        }
    }

    fn flag(self) -> Modifiers {
        match self {
            Self::Ctrl => Modifiers::CONTROL,
            Self::Alt => Modifiers::ALT,
            Self::Shift => Modifiers::SHIFT,
            Self::Win => Modifiers::SUPER,
        }
    }
}

fn is_modifier_code(code: Code) -> bool {
    matches!(
        code,
        Code::ControlLeft
            | Code::ControlRight
            | Code::AltLeft
            | Code::AltRight
            | Code::ShiftLeft
            | Code::ShiftRight
            | Code::MetaLeft
            | Code::MetaRight
    )
}

fn allowed_without_modifier(code: Code) -> bool {
    use Code::*;
    matches!(
        code,
        PrintScreen
            | Pause
            | ScrollLock
            | F1
            | F2
            | F3
            | F4
            | F5
            | F6
            | F7
            | F8
            | F9
            | F10
            | F11
            | F12
            | F13
            | F14
            | F15
            | F16
            | F17
            | F18
            | F19
            | F20
            | F21
            | F22
            | F23
            | F24
    )
}

struct Parts {
    modifiers: Vec<Modifier>,
    code: Code,
}

fn split(input: &str) -> Result<Parts, InvalidAccelerator> {
    let mut modifiers = Vec::new();
    let mut code = None;
    for token in input.split('+').map(str::trim).filter(|t| !t.is_empty()) {
        if let Some(m) = Modifier::parse(token) {
            if !modifiers.contains(&m) {
                modifiers.push(m);
            }
            continue;
        }
        let parsed = Code::from_str(token).map_err(|_| InvalidAccelerator::UnknownKey)?;
        if parsed == Code::Unidentified || code.is_some() {
            return Err(InvalidAccelerator::UnknownKey);
        }
        if is_modifier_code(parsed) {
            // A lone modifier key counts as "no key".
            continue;
        }
        code = Some(parsed);
    }
    let code = code.ok_or(InvalidAccelerator::NoKey)?;
    if modifiers.is_empty() && !allowed_without_modifier(code) {
        return Err(InvalidAccelerator::ModifierRequired);
    }
    modifiers.sort_by_key(|m| Modifier::ALL.iter().position(|x| x == m));
    Ok(Parts { modifiers, code })
}

/// Validates `input` and returns it in normalized form.
pub fn parse(input: &str) -> Result<Accelerator, InvalidAccelerator> {
    let Parts { modifiers, code } = split(input)?;
    let mut normalized: Vec<&str> = modifiers.iter().map(|m| m.name()).collect();
    let code_name = code.to_string();
    normalized.push(&code_name);
    Ok(Accelerator::from_normalized(normalized.join("+")))
}

/// Converts a normalized accelerator into a global-shortcut `Shortcut`.
pub fn to_shortcut(accelerator: &Accelerator) -> Result<Shortcut, InvalidAccelerator> {
    let Parts { modifiers, code } = split(accelerator.as_str())?;
    let flags = modifiers
        .iter()
        .fold(Modifiers::empty(), |acc, m| acc | m.flag());
    Ok(Shortcut::new((!flags.is_empty()).then_some(flags), code))
}

/// User-facing text, e.g. `"Ctrl + PrintScreen"`, `"Ctrl + Shift + A"`.
pub fn display(accelerator: &Accelerator) -> String {
    accelerator
        .as_str()
        .split('+')
        .map(display_token)
        .collect::<Vec<_>>()
        .join(" + ")
}

fn display_token(token: &str) -> &str {
    if let Some(letter) = token.strip_prefix("Key") {
        if letter.len() == 1 {
            return letter;
        }
    }
    if let Some(digit) = token.strip_prefix("Digit") {
        if digit.len() == 1 {
            return digit;
        }
    }
    token
}

#[cfg(test)]
mod tests {
    use super::*;

    fn norm(input: &str) -> String {
        parse(input).unwrap().as_str().to_owned()
    }

    #[test]
    fn keeps_normalized_accelerators() {
        for s in [
            "PrintScreen",
            "Ctrl+PrintScreen",
            "Shift+PrintScreen",
            "Ctrl+Shift+KeyA",
            "F9",
        ] {
            assert_eq!(norm(s), s);
        }
    }

    #[test]
    fn orders_and_dedupes_modifiers() {
        assert_eq!(norm("Shift+Win+Ctrl+Alt+KeyS"), "Ctrl+Alt+Shift+Win+KeyS");
        assert_eq!(norm("control + shift + KeyA"), "Ctrl+Shift+KeyA");
        assert_eq!(norm("Ctrl+Ctrl+PrintScreen"), "Ctrl+PrintScreen");
        assert_eq!(norm("Meta+Digit1"), "Win+Digit1");
    }

    #[test]
    fn rejects_invalid_input() {
        assert_eq!(parse(""), Err(InvalidAccelerator::NoKey));
        assert_eq!(parse("Ctrl+Shift"), Err(InvalidAccelerator::NoKey));
        assert_eq!(parse("Ctrl+ControlLeft"), Err(InvalidAccelerator::NoKey));
        assert_eq!(parse("KeyA"), Err(InvalidAccelerator::ModifierRequired));
        assert_eq!(parse("Space"), Err(InvalidAccelerator::ModifierRequired));
        assert_eq!(parse("Ctrl+Banana"), Err(InvalidAccelerator::UnknownKey));
        assert_eq!(parse("Ctrl+KeyA+KeyB"), Err(InvalidAccelerator::UnknownKey));
        assert_eq!(parse("printscreen"), Err(InvalidAccelerator::UnknownKey));
    }

    #[test]
    fn allows_function_and_system_keys_without_modifier() {
        for s in ["PrintScreen", "Pause", "ScrollLock", "F1", "F24"] {
            assert_eq!(norm(s), s);
        }
    }

    #[test]
    fn converts_to_shortcut() {
        let plain = to_shortcut(&parse("PrintScreen").unwrap()).unwrap();
        assert_eq!(plain, Shortcut::new(None, Code::PrintScreen));
        let ctrl = to_shortcut(&parse("Ctrl+PrintScreen").unwrap()).unwrap();
        assert_eq!(
            ctrl,
            Shortcut::new(Some(Modifiers::CONTROL), Code::PrintScreen)
        );
        let combo = to_shortcut(&parse("Ctrl+Shift+Win+KeyA").unwrap()).unwrap();
        assert_eq!(
            combo,
            Shortcut::new(
                Some(Modifiers::CONTROL | Modifiers::SHIFT | Modifiers::SUPER),
                Code::KeyA
            )
        );
    }

    #[test]
    fn displays_for_users() {
        let d = |s: &str| display(&parse(s).unwrap());
        assert_eq!(d("PrintScreen"), "PrintScreen");
        assert_eq!(d("Ctrl+PrintScreen"), "Ctrl + PrintScreen");
        assert_eq!(d("Ctrl+Shift+KeyA"), "Ctrl + Shift + A");
        assert_eq!(d("Alt+Digit3"), "Alt + 3");
    }
}
