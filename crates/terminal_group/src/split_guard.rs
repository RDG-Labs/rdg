use workspace::SplitDirection;

/// How a terminal group responds to a split that cannot produce a usable tile.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Default)]
pub enum SplitGuard {
    /// Try the perpendicular axis before refusing.
    #[default]
    Adapt,
    /// Refuse without trying the other axis.
    Refuse,
    /// Never refuse. Matches Wave Terminal, which will happily split a tile
    /// down to a few unreadable columns.
    Off,
}

/// The geometry a split decision is made against. Sizes are pixels; a tile's
/// usable area is its box minus one header.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct TileMetrics {
    pub width: f32,
    pub height: f32,
    pub cell_width: f32,
    pub line_height: f32,
    /// Gutter introduced between the two halves.
    pub gap: f32,
    pub header_height: f32,
}

/// The usability floor, expressed in character cells rather than pixels so it
/// holds across font sizes.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct TileMinimum {
    pub columns: usize,
    pub rows: usize,
}

impl Default for TileMinimum {
    fn default() -> Self {
        Self {
            columns: 30,
            rows: 8,
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum SplitOutcome {
    /// Perform the split along this axis. It may differ from the requested
    /// direction when the guard adapted.
    Split(SplitDirection),
    /// There is no room. No tile is created and no shell is spawned.
    Refused,
}

impl TileMetrics {
    /// Whether a box of this size seats a usable terminal.
    fn fits(&self, width: f32, height: f32, minimum: TileMinimum) -> bool {
        if self.cell_width <= 0. || self.line_height <= 0. {
            return false;
        }
        let columns = (width / self.cell_width).floor();
        let rows = ((height - self.header_height) / self.line_height).floor();
        columns >= minimum.columns as f32 && rows >= minimum.rows as f32
    }

    /// Whether halving this tile along `direction` leaves both halves usable.
    fn admits(&self, direction: SplitDirection, minimum: TileMinimum) -> bool {
        match direction {
            SplitDirection::Left | SplitDirection::Right => {
                let half = (self.width - self.gap) / 2.;
                self.fits(half, self.height, minimum)
            }
            SplitDirection::Up | SplitDirection::Down => {
                let half = (self.height - self.gap) / 2.;
                self.fits(self.width, half, minimum)
            }
        }
    }
}

/// Resolves a requested split against the usability floor.
///
/// A refusal is a real outcome, not an error: the caller must not create a tile
/// or spawn a shell. Spawning a process into a tile the user cannot read is the
/// harm this guard exists to prevent.
pub fn resolve_split(
    metrics: TileMetrics,
    requested: SplitDirection,
    minimum: TileMinimum,
    guard: SplitGuard,
) -> SplitOutcome {
    if guard == SplitGuard::Off {
        return SplitOutcome::Split(requested);
    }

    if metrics.admits(requested, minimum) {
        return SplitOutcome::Split(requested);
    }

    if guard == SplitGuard::Adapt {
        let perpendicular = match requested {
            SplitDirection::Left | SplitDirection::Right => SplitDirection::Down,
            SplitDirection::Up | SplitDirection::Down => SplitDirection::Right,
        };
        if metrics.admits(perpendicular, minimum) {
            return SplitOutcome::Split(perpendicular);
        }
    }

    SplitOutcome::Refused
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Roughly a 13px monospace face.
    fn metrics(width: f32, height: f32) -> TileMetrics {
        TileMetrics {
            width,
            height,
            cell_width: 8.,
            line_height: 18.,
            gap: 6.,
            header_height: 24.,
        }
    }

    const MIN: TileMinimum = TileMinimum {
        columns: 30,
        rows: 8,
    };

    #[test]
    fn splits_when_both_halves_fit() {
        // 1200px wide halves to 597px, well past 30 columns (240px).
        let outcome = resolve_split(
            metrics(1200., 800.),
            SplitDirection::Right,
            MIN,
            SplitGuard::Adapt,
        );
        assert_eq!(outcome, SplitOutcome::Split(SplitDirection::Right));
    }

    #[test]
    fn adapts_to_the_perpendicular_axis_when_width_runs_out() {
        // 400px halves to 197px = 24 columns, under the floor. Height has room
        // for two tiles, so the split turns downward instead of being lost.
        let outcome = resolve_split(
            metrics(400., 800.),
            SplitDirection::Right,
            MIN,
            SplitGuard::Adapt,
        );
        assert_eq!(outcome, SplitOutcome::Split(SplitDirection::Down));
    }

    #[test]
    fn refuses_when_neither_axis_fits() {
        // This is the Wave failure: a tile with no room in either direction.
        let outcome = resolve_split(
            metrics(400., 200.),
            SplitDirection::Right,
            MIN,
            SplitGuard::Adapt,
        );
        assert_eq!(outcome, SplitOutcome::Refused);
    }

    #[test]
    fn refuse_mode_does_not_adapt() {
        let outcome = resolve_split(
            metrics(400., 800.),
            SplitDirection::Right,
            MIN,
            SplitGuard::Refuse,
        );
        assert_eq!(outcome, SplitOutcome::Refused);
    }

    #[test]
    fn off_mode_always_splits() {
        let outcome = resolve_split(
            metrics(40., 40.),
            SplitDirection::Right,
            MIN,
            SplitGuard::Off,
        );
        assert_eq!(outcome, SplitOutcome::Split(SplitDirection::Right));
    }

    #[test]
    fn header_and_gap_are_charged_against_a_vertical_split() {
        // 2 headers (48) + gap (6) + 16 rows (288) = 342. At 340 the split must
        // not fit; at 344 it must.
        assert_eq!(
            resolve_split(
                metrics(1200., 340.),
                SplitDirection::Down,
                MIN,
                SplitGuard::Refuse
            ),
            SplitOutcome::Refused
        );
        assert_eq!(
            resolve_split(
                metrics(1200., 344.),
                SplitDirection::Down,
                MIN,
                SplitGuard::Refuse
            ),
            SplitOutcome::Split(SplitDirection::Down)
        );
    }

    #[test]
    fn degenerate_font_metrics_refuse_rather_than_divide_by_zero() {
        let mut degenerate = metrics(1200., 800.);
        degenerate.cell_width = 0.;
        assert_eq!(
            resolve_split(degenerate, SplitDirection::Right, MIN, SplitGuard::Adapt),
            SplitOutcome::Refused
        );
    }

    /// Ship gate 4: ten consecutive splits from one tile in a 1400x900 window
    /// must produce a usable grid or a refusal, never an unusable tile.
    #[test]
    fn repeated_splits_never_produce_an_unusable_tile() {
        let mut width = 1400_f32;
        let mut height = 900_f32;
        let mut refusals = 0;

        for _ in 0..10 {
            let current = metrics(width, height);
            match resolve_split(current, SplitDirection::Right, MIN, SplitGuard::Adapt) {
                SplitOutcome::Split(direction) => {
                    match direction {
                        SplitDirection::Left | SplitDirection::Right => width = (width - 6.) / 2.,
                        SplitDirection::Up | SplitDirection::Down => height = (height - 6.) / 2.,
                    }
                    // Every tile the guard admits is usable after the split.
                    assert!(
                        metrics(width, height).fits(width, height, MIN),
                        "guard admitted an unusable {width}x{height} tile"
                    );
                }
                SplitOutcome::Refused => refusals += 1,
            }
        }

        assert!(
            refusals > 0,
            "ten splits from one tile must eventually be refused"
        );
    }
}
