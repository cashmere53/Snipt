//! Window mode: which top-level windows can be captured, and which one is under the cursor.
//! The Windows adapter reads each window's state; the rules here decide without the OS.

use super::geometry::{PhysicalPoint, PhysicalRect};

/// A capturable window.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WindowCandidate {
    /// The `HWND`, as an integer so that this module stays OS independent.
    pub hwnd: isize,
    /// The visible frame (without the invisible resize borders), global physical pixels.
    pub rect: PhysicalRect,
    pub title: String,
    /// 0 is the topmost.
    pub z: u32,
}

/// A top-level window as the OS reports it, in z order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TopLevelWindow {
    pub hwnd: isize,
    pub rect: PhysicalRect,
    pub title: String,
    pub class_name: String,
    pub visible: bool,
    pub minimized: bool,
    /// Hidden by DWM: on another virtual desktop, or a suspended UWP app.
    pub cloaked: bool,
}

/// The desktop and the taskbars: always under the cursor somewhere, never what is wanted.
const SHELL_CLASSES: [&str; 4] = [
    "Progman",
    "WorkerW",
    "Shell_TrayWnd",
    "Shell_SecondaryTrayWnd",
];

/// Whether `window` can be picked. `own` are Snipt's overlays and backdrops.
pub fn is_capturable(window: &TopLevelWindow, own: &[isize]) -> bool {
    window.visible
        && !window.minimized
        && !window.cloaked
        && !window.rect.is_empty()
        && !own.contains(&window.hwnd)
        && !SHELL_CLASSES.contains(&window.class_name.as_str())
}

/// The capturable windows of `windows` (topmost first), numbered from 0 in that order.
pub fn candidates(windows: Vec<TopLevelWindow>, own: &[isize]) -> Vec<WindowCandidate> {
    windows
        .into_iter()
        .filter(|window| is_capturable(window, own))
        .zip(0..)
        .map(|(window, z)| WindowCandidate {
            hwnd: window.hwnd,
            rect: window.rect,
            title: window.title,
            z,
        })
        .collect()
}

/// The topmost candidate containing `p`.
pub fn hit_test(candidates: &[WindowCandidate], p: PhysicalPoint) -> Option<&WindowCandidate> {
    candidates
        .iter()
        .filter(|candidate| candidate.rect.contains(p))
        .min_by_key(|candidate| candidate.z)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn window(hwnd: isize, rect: PhysicalRect) -> TopLevelWindow {
        TopLevelWindow {
            hwnd,
            rect,
            title: format!("window {hwnd}"),
            class_name: "App".to_owned(),
            visible: true,
            minimized: false,
            cloaked: false,
        }
    }

    #[test]
    fn picks_the_topmost_of_overlapping_windows() {
        let list = candidates(
            vec![
                window(1, PhysicalRect::new(100, 100, 200, 200)),
                window(2, PhysicalRect::new(0, 0, 1000, 800)),
            ],
            &[],
        );
        let hit = |x, y| hit_test(&list, PhysicalPoint::new(x, y)).map(|c| c.hwnd);
        assert_eq!(hit(150, 150), Some(1));
        assert_eq!(hit(50, 50), Some(2));
        assert_eq!(hit(1500, 50), None);
        // Candidates in any order: z decides.
        let reversed: Vec<_> = list.iter().rev().cloned().collect();
        assert_eq!(
            hit_test(&reversed, PhysicalPoint::new(150, 150)).map(|c| c.hwnd),
            Some(1)
        );
    }

    #[test]
    fn leaves_out_windows_that_cannot_be_captured() {
        let rect = PhysicalRect::new(0, 0, 500, 500);
        let excluded = vec![
            TopLevelWindow {
                visible: false,
                ..window(1, rect)
            },
            TopLevelWindow {
                minimized: true,
                ..window(2, rect)
            },
            TopLevelWindow {
                cloaked: true,
                ..window(3, rect)
            },
            window(4, PhysicalRect::new(0, 0, 0, 500)),
            // Snipt's own overlay.
            window(5, rect),
            TopLevelWindow {
                class_name: "Shell_TrayWnd".to_owned(),
                ..window(6, rect)
            },
            TopLevelWindow {
                class_name: "Progman".to_owned(),
                ..window(7, rect)
            },
        ];
        let mut all = excluded;
        all.push(window(8, rect));
        let list = candidates(all, &[5]);
        assert_eq!(
            list,
            [WindowCandidate {
                hwnd: 8,
                rect,
                title: "window 8".to_owned(),
                z: 0,
            }]
        );
        // The excluded windows above it are never highlighted.
        assert_eq!(
            hit_test(&list, PhysicalPoint::new(10, 10)).map(|c| c.hwnd),
            Some(8)
        );
    }
}
