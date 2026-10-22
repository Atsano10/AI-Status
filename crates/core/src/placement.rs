//! Where the bar goes on a display: the default spot, snapping after a drag, and keeping it on screen.
//!
//! Everything is in points with the origin at the top-left (like the page, unlike AppKit), and `area` is the
//! display's usable part: no menu bar, no Dock.

/// A rectangle: top-left corner plus size.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Rect {
    fn right(&self) -> f64 {
        self.x + self.width
    }
    fn bottom(&self) -> f64 {
        self.y + self.height
    }
    fn center_x(&self) -> f64 {
        self.x + self.width / 2.0
    }
}

/// The gap the bar keeps from an edge it's snapped to. Same as the default spot under the menu bar.
pub const MARGIN: f64 = 6.0;
/// Dropped this close (points) to a snap spot, the bar jumps to it.
pub const SNAP_DISTANCE: f64 = 18.0;

/// The default spot: top centre, just under the menu bar (where the notch is on MacBooks).
pub fn top_center(width: f64, area: Rect) -> (f64, f64) {
    (area.center_x() - width / 2.0, area.y + MARGIN)
}

/// Where a bar dropped at `bar` ends up. Near an edge it moves to `MARGIN` from it (two edges make a corner);
/// near the top centre it centres under the notch. It's always kept fully on the display.
pub fn snap(bar: Rect, area: Rect) -> (f64, f64) {
    let mut x = bar.x;
    let mut y = bar.y;

    let near = |gap: f64| gap < MARGIN + SNAP_DISTANCE; // a negative gap (hanging off the edge) counts too
    if near(bar.x - area.x) {
        x = area.x + MARGIN;
    } else if near(area.right() - bar.right()) {
        x = area.right() - MARGIN - bar.width;
    }
    let top = near(bar.y - area.y);
    if top {
        y = area.y + MARGIN;
    } else if near(area.bottom() - bar.bottom()) {
        y = area.bottom() - MARGIN - bar.height;
    }
    if top && (bar.center_x() - area.center_x()).abs() < SNAP_DISTANCE {
        x = top_center(bar.width, area).0;
    }

    keep_inside(Rect { x, y, ..bar }, area)
}

/// Moves `bar` the least it can so it's fully on the display (for a saved spot on a display that got smaller).
pub fn keep_inside(bar: Rect, area: Rect) -> (f64, f64) {
    // `max` first: a bar bigger than the display sticks to the top-left instead of going off it.
    let x = bar.x.min(area.right() - bar.width).max(area.x);
    let y = bar.y.min(area.bottom() - bar.height).max(area.y);
    (x, y)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 1512×945 MacBook screen minus a 37pt menu bar.
    const AREA: Rect = Rect {
        x: 0.0,
        y: 37.0,
        width: 1512.0,
        height: 945.0 - 37.0,
    };

    fn bar(x: f64, y: f64) -> Rect {
        Rect {
            x,
            y,
            width: 392.0,
            height: 54.0,
        }
    }

    #[test]
    fn default_spot_is_top_centre() {
        assert_eq!(top_center(392.0, AREA), (560.0, 43.0));
    }

    #[test]
    fn far_from_edges_stays_put() {
        assert_eq!(snap(bar(300.0, 400.0), AREA), (300.0, 400.0));
    }

    #[test]
    fn near_an_edge_snaps_to_it() {
        assert_eq!(snap(bar(10.0, 400.0), AREA), (MARGIN, 400.0)); // left
        let right = AREA.width - 392.0 - 15.0;
        assert_eq!(
            snap(bar(right, 400.0), AREA),
            (AREA.width - MARGIN - 392.0, 400.0)
        );
    }

    #[test]
    fn near_a_corner_snaps_to_both_edges() {
        let bottom = AREA.y + AREA.height - 54.0 - 10.0;
        assert_eq!(
            snap(bar(8.0, bottom), AREA),
            (MARGIN, AREA.y + AREA.height - MARGIN - 54.0)
        );
    }

    #[test]
    fn near_the_top_centre_docks_under_the_notch() {
        assert_eq!(snap(bar(570.0, 50.0), AREA), top_center(392.0, AREA));
    }

    #[test]
    fn off_screen_comes_back() {
        assert_eq!(snap(bar(-200.0, 400.0), AREA), (MARGIN, 400.0));
        assert_eq!(keep_inside(bar(2000.0, 2000.0), AREA), (1120.0, 891.0));
    }
}
