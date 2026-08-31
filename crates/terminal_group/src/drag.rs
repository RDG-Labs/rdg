//! Drop-zone geometry for rearranging tiles by dragging.
//!
//! Three semantics, reproduced from Wave Terminal and adopted deliberately:
//! the centre of a tile swaps, an edge band inserts a sibling inside the
//! target's own axis, and a band along the group's outer bounds inserts at the
//! root axis — which is how a tile is promoted out of a nested column.
//!
//! Everything here is pure arithmetic on pixel rectangles so it can be tested
//! without a window.

use workspace::SplitDirection;

/// Fraction of a tile, on each side, that reads as an edge rather than centre.
/// The remaining inner 50% x 50% is the swap target.
const EDGE_BAND: f32 = 0.25;

/// Distance from the group's outer bounds that promotes a drop to the root axis.
const OUTER_BAND_PX: f32 = 24.;

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Rect {
    pub fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    fn right(&self) -> f32 {
        self.x + self.width
    }

    fn bottom(&self) -> f32 {
        self.y + self.height
    }

    fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.x && x <= self.right() && y >= self.y && y <= self.bottom()
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum DropZone {
    /// Exchange the dragged tile with the target. Sizes stay with the
    /// positions, not with the tiles.
    Swap,
    /// Insert beside the target, inside the target's own axis.
    Edge(SplitDirection),
    /// Insert as a child of the root axis: a full-height column or
    /// full-width row.
    Outer(SplitDirection),
}

/// Resolves a pointer position to a drop zone.
///
/// `group` is the whole grid's bounds and `tile` the bounds of the tile under
/// the pointer. Both are in the same coordinate space; which space does not
/// matter, only that they agree.
pub fn zone_at(x: f32, y: f32, tile: Rect, group: Rect) -> DropZone {
    // The outer band wins over everything: it is the only way to promote a
    // deeply nested tile back out to the root.
    if let Some(direction) = outer_edge(x, y, group) {
        return DropZone::Outer(direction);
    }

    if tile.width <= 0. || tile.height <= 0. {
        return DropZone::Swap;
    }

    let horizontal = ((x - tile.x) / tile.width).clamp(0., 1.);
    let vertical = ((y - tile.y) / tile.height).clamp(0., 1.);

    let horizontal_depth = horizontal.min(1. - horizontal);
    let vertical_depth = vertical.min(1. - vertical);

    let in_horizontal_band = horizontal_depth < EDGE_BAND;
    let in_vertical_band = vertical_depth < EDGE_BAND;

    match (in_horizontal_band, in_vertical_band) {
        (false, false) => DropZone::Swap,
        // In a corner, the nearer edge wins, so the zone matches the edge the
        // pointer is actually hugging.
        (true, true) if horizontal_depth <= vertical_depth => {
            DropZone::Edge(horizontal_direction(horizontal))
        }
        (true, true) => DropZone::Edge(vertical_direction(vertical)),
        (true, false) => DropZone::Edge(horizontal_direction(horizontal)),
        (false, true) => DropZone::Edge(vertical_direction(vertical)),
    }
}

fn horizontal_direction(fraction: f32) -> SplitDirection {
    if fraction < 0.5 {
        SplitDirection::Left
    } else {
        SplitDirection::Right
    }
}

fn vertical_direction(fraction: f32) -> SplitDirection {
    if fraction < 0.5 {
        SplitDirection::Up
    } else {
        SplitDirection::Down
    }
}

fn outer_edge(x: f32, y: f32, group: Rect) -> Option<SplitDirection> {
    if !group.contains(x, y) {
        return None;
    }

    let distances = [
        (x - group.x, SplitDirection::Left),
        (group.right() - x, SplitDirection::Right),
        (y - group.y, SplitDirection::Up),
        (group.bottom() - y, SplitDirection::Down),
    ];

    distances
        .into_iter()
        .filter(|(distance, _)| *distance <= OUTER_BAND_PX)
        .min_by(|(a, _), (b, _)| a.total_cmp(b))
        .map(|(_, direction)| direction)
}

/// The rectangle to highlight for a zone.
///
/// A filled region rather than a thin line: it shows the shape the tile will
/// actually occupy, which reads far more clearly than an insertion caret.
pub fn preview_rect(zone: DropZone, tile: Rect, group: Rect) -> Rect {
    match zone {
        DropZone::Swap => tile,
        DropZone::Edge(direction) => half_of(tile, direction),
        DropZone::Outer(direction) => band_of(group, direction),
    }
}

fn half_of(rect: Rect, direction: SplitDirection) -> Rect {
    match direction {
        SplitDirection::Left => Rect::new(rect.x, rect.y, rect.width / 2., rect.height),
        SplitDirection::Right => Rect::new(
            rect.x + rect.width / 2.,
            rect.y,
            rect.width / 2.,
            rect.height,
        ),
        SplitDirection::Up => Rect::new(rect.x, rect.y, rect.width, rect.height / 2.),
        SplitDirection::Down => Rect::new(
            rect.x,
            rect.y + rect.height / 2.,
            rect.width,
            rect.height / 2.,
        ),
    }
}

/// A root-axis insert takes a third of the group along the relevant edge, which
/// is roughly what it will measure once the tree re-balances.
fn band_of(group: Rect, direction: SplitDirection) -> Rect {
    let horizontal = group.width / 3.;
    let vertical = group.height / 3.;
    match direction {
        SplitDirection::Left => Rect::new(group.x, group.y, horizontal, group.height),
        SplitDirection::Right => Rect::new(
            group.right() - horizontal,
            group.y,
            horizontal,
            group.height,
        ),
        SplitDirection::Up => Rect::new(group.x, group.y, group.width, vertical),
        SplitDirection::Down => Rect::new(
            group.x,
            group.bottom() - vertical,
            group.width,
            vertical,
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 900x600 group whose middle tile occupies the centre third.
    fn group() -> Rect {
        Rect::new(0., 0., 900., 600.)
    }

    fn middle_tile() -> Rect {
        Rect::new(300., 200., 300., 200.)
    }

    #[test]
    fn the_middle_of_a_tile_swaps() {
        assert_eq!(zone_at(450., 300., middle_tile(), group()), DropZone::Swap);
    }

    #[test]
    fn each_edge_band_inserts_on_that_side() {
        let tile = middle_tile();
        // 320 is 6.7% into a 300-wide tile: inside the 25% left band.
        assert_eq!(
            zone_at(320., 300., tile, group()),
            DropZone::Edge(SplitDirection::Left)
        );
        assert_eq!(
            zone_at(580., 300., tile, group()),
            DropZone::Edge(SplitDirection::Right)
        );
        assert_eq!(
            zone_at(450., 210., tile, group()),
            DropZone::Edge(SplitDirection::Up)
        );
        assert_eq!(
            zone_at(450., 390., tile, group()),
            DropZone::Edge(SplitDirection::Down)
        );
    }

    #[test]
    fn a_corner_resolves_to_the_nearer_edge() {
        let tile = middle_tile();
        // 5px into the left edge, 20px into the top: left is nearer in
        // fractional terms (0.017 vs 0.1).
        assert_eq!(
            zone_at(305., 220., tile, group()),
            DropZone::Edge(SplitDirection::Left)
        );
        // 60px in horizontally (0.2) but 2px down (0.01): top wins.
        assert_eq!(
            zone_at(360., 202., tile, group()),
            DropZone::Edge(SplitDirection::Up)
        );
    }

    #[test]
    fn the_group_border_promotes_to_the_root_axis() {
        let tile = Rect::new(0., 0., 300., 600.);
        assert_eq!(
            zone_at(8., 300., tile, group()),
            DropZone::Outer(SplitDirection::Left)
        );
        assert_eq!(
            zone_at(896., 300., Rect::new(600., 0., 300., 600.), group()),
            DropZone::Outer(SplitDirection::Right)
        );
        assert_eq!(
            zone_at(450., 4., Rect::new(300., 0., 300., 600.), group()),
            DropZone::Outer(SplitDirection::Up)
        );
    }

    #[test]
    fn the_outer_band_beats_the_edge_band() {
        // Hugging the group's left border, inside a tile that also starts there.
        let tile = Rect::new(0., 0., 300., 600.);
        assert_eq!(
            zone_at(2., 300., tile, group()),
            DropZone::Outer(SplitDirection::Left)
        );
        // Just outside the outer band, the tile's own edge band takes over.
        assert_eq!(
            zone_at(30., 300., tile, group()),
            DropZone::Edge(SplitDirection::Left)
        );
    }

    #[test]
    fn a_corner_of_the_group_picks_the_closest_border() {
        // 4px from the top, 10px from the left: top is nearer.
        assert_eq!(
            zone_at(10., 4., Rect::new(0., 0., 300., 600.), group()),
            DropZone::Outer(SplitDirection::Up)
        );
    }

    #[test]
    fn a_swap_preview_covers_the_whole_target() {
        assert_eq!(
            preview_rect(DropZone::Swap, middle_tile(), group()),
            middle_tile()
        );
    }

    #[test]
    fn an_edge_preview_covers_the_half_the_tile_will_take() {
        assert_eq!(
            preview_rect(DropZone::Edge(SplitDirection::Right), middle_tile(), group()),
            Rect::new(450., 200., 150., 200.)
        );
        assert_eq!(
            preview_rect(DropZone::Edge(SplitDirection::Up), middle_tile(), group()),
            Rect::new(300., 200., 300., 100.)
        );
    }

    #[test]
    fn an_outer_preview_spans_the_group() {
        assert_eq!(
            preview_rect(DropZone::Outer(SplitDirection::Left), middle_tile(), group()),
            Rect::new(0., 0., 300., 600.)
        );
        assert_eq!(
            preview_rect(DropZone::Outer(SplitDirection::Down), middle_tile(), group()),
            Rect::new(0., 400., 900., 200.)
        );
    }

    #[test]
    fn a_degenerate_tile_falls_back_to_swap() {
        let empty = Rect::new(400., 300., 0., 0.);
        assert_eq!(zone_at(400., 300., empty, group()), DropZone::Swap);
    }
}
