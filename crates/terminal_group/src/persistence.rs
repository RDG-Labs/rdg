//! Layout persistence for terminal groups.
//!
//! v1 stores shape only: the tile tree, its proportions, and which tile was
//! focused or magnified. Working directories, scrollback, and shell state are
//! deliberately not restored — see the PRD's non-goals. Restored tiles spawn
//! their shells lazily, so reopening a twelve-tile grid costs one process
//! rather than twelve.

use db::{
    query,
    sqlez::{domain::Domain, thread_safe_connection::ThreadSafeConnection},
    sqlez_macros::sql,
};
use gpui::{App, Axis};
use serde::{Deserialize, Serialize};
use workspace::{ItemId, WorkspaceDb, WorkspaceId};

/// Bumped whenever the stored shape changes meaning. A layout written by a
/// newer build restores as a single tile rather than crashing or silently
/// dropping tiles.
pub(crate) const LAYOUT_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(crate) struct SerializedTerminalGroup {
    pub version: u32,
    pub root: SerializedTileTree,
    /// Index of the focused tile in reading order, if any.
    pub focused_tile: Option<usize>,
    /// Index of the magnified tile in reading order, if any.
    pub magnified_tile: Option<usize>,
    pub title: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum SerializedTileTree {
    Axis {
        axis: SerializedAxis,
        flexes: Vec<f32>,
        children: Vec<SerializedTileTree>,
    },
    Tile(SerializedTile),
}

impl SerializedTileTree {
    /// Tiles in reading order, matching how focus indices are recorded.
    pub(crate) fn tile_count(&self) -> usize {
        match self {
            SerializedTileTree::Tile(_) => 1,
            SerializedTileTree::Axis { children, .. } => {
                children.iter().map(SerializedTileTree::tile_count).sum()
            }
        }
    }

    /// Whether the tree is well formed: every axis has as many flexes as
    /// children, and no axis is empty. A malformed tree is discarded rather
    /// than restored into a broken grid.
    pub(crate) fn is_well_formed(&self) -> bool {
        match self {
            SerializedTileTree::Tile(_) => true,
            SerializedTileTree::Axis {
                flexes, children, ..
            } => {
                !children.is_empty()
                    && flexes.len() == children.len()
                    && flexes.iter().all(|flex| flex.is_finite() && *flex > 0.)
                    && children.iter().all(SerializedTileTree::is_well_formed)
            }
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub(crate) struct SerializedTile {
    /// Reserved for the phase that restores working directories. Written as
    /// `None` today so a later build reading a v1 layout needs no migration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub working_directory: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title_override: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SerializedAxis {
    Horizontal,
    Vertical,
}

impl From<Axis> for SerializedAxis {
    fn from(axis: Axis) -> Self {
        match axis {
            Axis::Horizontal => SerializedAxis::Horizontal,
            Axis::Vertical => SerializedAxis::Vertical,
        }
    }
}

impl From<SerializedAxis> for Axis {
    fn from(axis: SerializedAxis) -> Self {
        match axis {
            SerializedAxis::Horizontal => Axis::Horizontal,
            SerializedAxis::Vertical => Axis::Vertical,
        }
    }
}

pub struct TerminalGroupDb(ThreadSafeConnection);

impl Domain for TerminalGroupDb {
    const NAME: &str = stringify!(TerminalGroupDb);

    const MIGRATIONS: &[&str] = &[sql!(
        CREATE TABLE terminal_groups (
            workspace_id INTEGER,
            item_id INTEGER,
            layout TEXT,
            PRIMARY KEY(workspace_id, item_id),
            FOREIGN KEY(workspace_id) REFERENCES workspaces(workspace_id)
            ON DELETE CASCADE
        ) STRICT;
    )];
}

db::static_connection!(TerminalGroupDb, [WorkspaceDb]);

impl TerminalGroupDb {
    query! {
        pub async fn save_layout(
            item_id: ItemId,
            workspace_id: WorkspaceId,
            layout: String
        ) -> Result<()> {
            INSERT OR REPLACE INTO terminal_groups(item_id, workspace_id, layout)
            SELECT candidate.item_id, candidate.workspace_id, candidate.layout
            FROM (SELECT ? AS item_id, ? AS workspace_id, ? AS layout) AS candidate
            WHERE candidate.workspace_id IN (
                SELECT workspace_id FROM workspaces
            )
        }
    }

    query! {
        pub fn get_layout(item_id: ItemId, workspace_id: WorkspaceId) -> Result<Option<String>> {
            SELECT layout
            FROM terminal_groups
            WHERE item_id = ? AND workspace_id = ?
        }
    }
}

/// Reads a stored layout, returning `None` when there is nothing usable.
///
/// An unreadable, malformed, or future-versioned layout is treated as absent so
/// the group opens with a single tile instead of failing to open at all.
pub(crate) fn load_layout(
    item_id: ItemId,
    workspace_id: WorkspaceId,
    max_tiles: usize,
    cx: &App,
) -> Option<SerializedTerminalGroup> {
    let raw = TerminalGroupDb::global(cx)
        .get_layout(item_id, workspace_id)
        .ok()??;

    let layout: SerializedTerminalGroup = match serde_json::from_str(&raw) {
        Ok(layout) => layout,
        Err(error) => {
            log::warn!("discarding unreadable terminal group layout: {error:#}");
            return None;
        }
    };

    if layout.version > LAYOUT_VERSION {
        log::warn!(
            "terminal group layout version {} is newer than {LAYOUT_VERSION}; opening a fresh group",
            layout.version
        );
        return None;
    }

    if !layout.root.is_well_formed() {
        log::warn!("discarding malformed terminal group layout");
        return None;
    }

    let tiles = layout.root.tile_count();
    if tiles > max_tiles {
        log::warn!("discarding terminal group layout with {tiles} tiles; the cap is {max_tiles}");
        return None;
    }

    Some(layout)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tile() -> SerializedTileTree {
        SerializedTileTree::Tile(SerializedTile::default())
    }

    #[test]
    fn round_trips_through_json() {
        let layout = SerializedTerminalGroup {
            version: LAYOUT_VERSION,
            root: SerializedTileTree::Axis {
                axis: SerializedAxis::Horizontal,
                flexes: vec![1.5, 0.5],
                children: vec![
                    tile(),
                    SerializedTileTree::Axis {
                        axis: SerializedAxis::Vertical,
                        flexes: vec![1., 1.],
                        children: vec![tile(), tile()],
                    },
                ],
            },
            focused_tile: Some(2),
            magnified_tile: None,
            title: Some("services".into()),
        };

        let encoded = serde_json::to_string(&layout).expect("failed to encode");
        let decoded: SerializedTerminalGroup =
            serde_json::from_str(&encoded).expect("failed to decode");

        assert_eq!(layout, decoded);
        assert_eq!(decoded.root.tile_count(), 3);
    }

    #[test]
    fn rejects_an_axis_whose_flexes_do_not_match_its_children() {
        let malformed = SerializedTileTree::Axis {
            axis: SerializedAxis::Horizontal,
            flexes: vec![1.],
            children: vec![tile(), tile()],
        };
        assert!(!malformed.is_well_formed());
    }

    #[test]
    fn rejects_an_empty_axis() {
        let empty = SerializedTileTree::Axis {
            axis: SerializedAxis::Vertical,
            flexes: vec![],
            children: vec![],
        };
        assert!(!empty.is_well_formed());
    }

    #[test]
    fn rejects_non_finite_flexes() {
        let broken = SerializedTileTree::Axis {
            axis: SerializedAxis::Vertical,
            flexes: vec![f32::NAN, 1.],
            children: vec![tile(), tile()],
        };
        assert!(!broken.is_well_formed());
    }

    #[test]
    fn a_single_tile_is_well_formed() {
        assert!(tile().is_well_formed());
        assert_eq!(tile().tile_count(), 1);
    }
}
