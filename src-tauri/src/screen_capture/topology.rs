//! The monitor layout: which monitors there are, where, and at what DPI. A capture compares
//! fingerprints of the layout to notice monitors added, moved or rescaled during a capture.

use std::hash::{DefaultHasher, Hash, Hasher};

use serde::{Deserialize, Serialize};

use super::geometry::{PhysicalPoint, PhysicalRect};

/// A monitor's position in `MonitorTopology::monitors`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct MonitorIndex(pub u32);

#[derive(Clone, Debug, PartialEq)]
pub struct MonitorInfo {
    pub index: MonitorIndex,
    /// `\\.\DISPLAY1` and the like.
    pub device_name: String,
    /// Global physical pixels.
    pub rect: PhysicalRect,
    pub dpi: u32,
    /// `dpi / 96`.
    pub scale: f64,
    pub is_primary: bool,
}

impl MonitorInfo {
    pub fn size(&self) -> (u32, u32) {
        (self.rect.width, self.rect.height)
    }

    /// A rectangle local to this monitor on the virtual desktop.
    pub fn to_global(&self, local: PhysicalRect) -> PhysicalRect {
        local.translate(self.rect.x, self.rect.y)
    }

    /// A global rectangle from this monitor's top-left corner.
    pub fn to_local(&self, global: PhysicalRect) -> PhysicalRect {
        global.translate(self.rect.x.wrapping_neg(), self.rect.y.wrapping_neg())
    }

    pub fn point_to_local(&self, global: PhysicalPoint) -> PhysicalPoint {
        PhysicalPoint::new(
            global.x.saturating_sub(self.rect.x),
            global.y.saturating_sub(self.rect.y),
        )
    }

    /// Squared distance from `p` to the nearest pixel of the monitor; 0 inside.
    fn distance_squared(&self, p: PhysicalPoint) -> i128 {
        let gap = |v: i64, low: i64, high: i64| {
            if v < low {
                low - v
            } else if v >= high {
                v - high + 1
            } else {
                0
            }
        };
        let dx = i128::from(gap(i64::from(p.x), self.rect.left(), self.rect.right()));
        let dy = i128::from(gap(i64::from(p.y), self.rect.top(), self.rect.bottom()));
        dx * dx + dy * dy
    }
}

/// Identifies a layout: changes when a monitor is added, removed, moved, resized or rescaled.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TopologyFingerprint(u64);

#[derive(Clone, Debug, Default, PartialEq)]
pub struct MonitorTopology {
    /// Ordered by top, then left; each `index` is the monitor's position here.
    pub monitors: Vec<MonitorInfo>,
}

impl MonitorTopology {
    /// Orders the monitors by top, then left, and numbers them in that order.
    pub fn new(mut monitors: Vec<MonitorInfo>) -> Self {
        monitors.sort_by_key(|m| (m.rect.y, m.rect.x));
        for (i, monitor) in monitors.iter_mut().enumerate() {
            monitor.index = MonitorIndex(u32::try_from(i).unwrap_or(u32::MAX));
        }
        Self { monitors }
    }

    pub fn get(&self, index: MonitorIndex) -> Option<&MonitorInfo> {
        self.monitors.get(usize::try_from(index.0).ok()?)
    }

    pub fn fingerprint(&self) -> TopologyFingerprint {
        let mut hasher = DefaultHasher::new();
        for monitor in &self.monitors {
            monitor.device_name.hash(&mut hasher);
            monitor.rect.hash(&mut hasher);
            monitor.dpi.hash(&mut hasher);
        }
        TopologyFingerprint(hasher.finish())
    }

    /// The monitor under `p`, or the nearest one when `p` is in a gap between monitors (a
    /// cursor can be reported there while monitors change). `None` without monitors.
    pub fn monitor_at(&self, p: PhysicalPoint) -> Option<&MonitorInfo> {
        self.monitors
            .iter()
            .find(|m| m.rect.contains(p))
            .or_else(|| self.monitors.iter().min_by_key(|m| m.distance_squared(p)))
    }

    /// The rectangle around every monitor; empty without monitors.
    pub fn virtual_bounds(&self) -> PhysicalRect {
        let mut monitors = self.monitors.iter().map(|m| m.rect);
        let first = monitors.next().unwrap_or_default();
        monitors.fold(first, |bounds, rect| bounds.union(&rect))
    }
}

#[cfg(test)]
pub mod testing {
    use super::*;

    /// A monitor at `(x, y)` of `width × height` physical pixels at `scale`.
    pub fn monitor(name: &str, x: i32, y: i32, width: u32, height: u32, scale: f64) -> MonitorInfo {
        MonitorInfo {
            index: MonitorIndex(0),
            device_name: format!(r"\\.\{name}"),
            rect: PhysicalRect::new(x, y, width, height),
            dpi: (96.0 * scale).round() as u32,
            scale,
            is_primary: x == 0 && y == 0,
        }
    }

    /// A 4K monitor at 150% on the left of a 1080p primary monitor at 100%, their tops aligned.
    pub fn mixed_dpi() -> MonitorTopology {
        MonitorTopology::new(vec![
            monitor("DISPLAY1", 0, 0, 1920, 1080, 1.0),
            monitor("DISPLAY2", -3840, 0, 3840, 2160, 1.5),
        ])
    }
}

#[cfg(test)]
mod tests {
    use super::testing::{mixed_dpi, monitor};
    use super::*;

    fn p(x: i32, y: i32) -> PhysicalPoint {
        PhysicalPoint::new(x, y)
    }

    #[test]
    fn orders_and_numbers_the_monitors_by_top_then_left() {
        let topology = MonitorTopology::new(vec![
            monitor("B", 1920, 0, 1920, 1080, 1.0),
            monitor("C", 0, -1080, 1920, 1080, 1.0),
            monitor("A", 0, 0, 1920, 1080, 1.0),
        ]);
        let names: Vec<_> = topology
            .monitors
            .iter()
            .map(|m| (m.index, m.device_name.as_str()))
            .collect();
        assert_eq!(
            names,
            [
                (MonitorIndex(0), r"\\.\C"),
                (MonitorIndex(1), r"\\.\A"),
                (MonitorIndex(2), r"\\.\B"),
            ]
        );
        assert_eq!(topology.get(MonitorIndex(2)).unwrap().device_name, r"\\.\B");
        assert!(topology.get(MonitorIndex(3)).is_none());
    }

    #[test]
    fn the_fingerprint_follows_position_size_and_scale() {
        let base = mixed_dpi();
        assert_eq!(base.fingerprint(), mixed_dpi().fingerprint());

        let mut rescaled = mixed_dpi();
        rescaled.monitors[0].dpi = 192;
        rescaled.monitors[0].scale = 2.0;
        assert_ne!(base.fingerprint(), rescaled.fingerprint());

        let mut moved = mixed_dpi();
        moved.monitors[1].rect = moved.monitors[1].rect.translate(0, 100);
        assert_ne!(base.fingerprint(), moved.fingerprint());

        let removed = MonitorTopology::new(vec![base.monitors[0].clone()]);
        assert_ne!(base.fingerprint(), removed.fingerprint());
    }

    #[test]
    fn finds_the_monitor_under_a_point_or_the_nearest_one() {
        let topology = MonitorTopology::new(vec![
            monitor("LEFT", -1920, 0, 1920, 1080, 1.0),
            monitor("RIGHT", 100, 0, 1920, 1080, 1.0),
        ]);
        let name = |at| &topology.monitor_at(at).unwrap().device_name;
        assert_eq!(name(p(-1, 500)), r"\\.\LEFT");
        assert_eq!(name(p(100, 500)), r"\\.\RIGHT");
        // In the 100 px gap between them.
        assert_eq!(name(p(20, 500)), r"\\.\LEFT");
        assert_eq!(name(p(80, 500)), r"\\.\RIGHT");
        // Below every monitor.
        assert_eq!(name(p(500, 5000)), r"\\.\RIGHT");
        assert!(MonitorTopology::default().monitor_at(p(0, 0)).is_none());
    }

    #[test]
    fn bounds_cover_monitors_at_negative_coordinates() {
        let topology = MonitorTopology::new(vec![
            monitor("A", 0, 0, 1920, 1080, 1.0),
            monitor("B", -3840, -500, 3840, 2160, 1.5),
            monitor("C", 1920, 200, 1280, 1024, 1.0),
        ]);
        assert_eq!(
            topology.virtual_bounds(),
            PhysicalRect::new(-3840, -500, 3840 + 1920 + 1280, 2160)
        );
        assert!(MonitorTopology::default().virtual_bounds().is_empty());
    }

    #[test]
    fn converts_between_local_and_global_on_a_monitor_left_of_the_primary() {
        let topology = mixed_dpi();
        let left = topology.monitor_at(p(-10, 10)).unwrap();
        let local = PhysicalRect::new(100, 50, 300, 200);
        let global = left.to_global(local);
        assert_eq!(global, PhysicalRect::new(-3740, 50, 300, 200));
        assert_eq!(left.to_local(global), local);
        assert_eq!(left.point_to_local(p(-3840, 0)), p(0, 0));
    }
}
