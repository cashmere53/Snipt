//! Reads the Windows "Use the Print screen key to open screen capture" setting.

use windows::core::w;
use windows::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD};

/// `Some(true)` when Windows reserves PrintScreen for the Snipping Tool, `Some(false)` when the
/// setting is off, `None` when the value is absent or unreadable.
///
/// Windows 11 treats an absent value as "on", so callers must not read `None` as "off".
pub fn print_screen_opens_snipping() -> Option<bool> {
    let mut data: u32 = 0;
    let mut size = std::mem::size_of::<u32>() as u32;
    // SAFETY: `data` and `size` are valid for writes and `size` matches the buffer.
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            w!(r"Control Panel\Keyboard"),
            w!("PrintScreenKeyForSnippingEnabled"),
            RRF_RT_REG_DWORD,
            None,
            Some(std::ptr::from_mut(&mut data).cast()),
            Some(&mut size),
        )
    };
    status.is_ok().then_some(data != 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn returns_without_panicking() {
        // The value depends on the machine; only check that the call completes.
        let value = print_screen_opens_snipping();
        assert!(matches!(value, None | Some(true) | Some(false)));
    }
}
