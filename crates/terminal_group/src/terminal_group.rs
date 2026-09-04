//! A tiled terminal workspace that lives as a single tab in the center pane.
//!
//! A [`TerminalGroup`] owns its own [`PaneGroup`] of tiles, mirroring the way
//! `TerminalPanel` owns a pane group inside a dock. Each tile is a `Pane`
//! holding exactly one terminal and rendering a slim header in place of a tab
//! strip, so the grid reads as a wall of terminals rather than a nest of tabs.

mod agent_discovery;
mod drag;
mod persistence;
mod split_guard;
mod tile;

pub use agent_discovery::{InstalledAgent, installed_agents};
pub use drag::{DropZone, Rect, preview_rect, zone_at};
pub use split_guard::{SplitGuard, SplitOutcome, TileMetrics, TileMinimum, resolve_split};

use anyhow::Result;
use gpui::{
    AnyElement, App, DismissEvent, DragMoveEvent, Entity, EventEmitter, FocusHandle, Focusable,
    Point, Subscription, Task, WeakEntity, WindowHandle, WindowOptions, actions,
};
use project::Project;
use serde::Serialize;
use settings::Settings as _;
use std::collections::{HashMap, HashSet};
use terminal_view::TerminalView;
use ui::{
    AlertModal, ContextMenu, IconButton, IconButtonShape, IconName, IconSize, PopoverMenu, Tooltip,
    prelude::*,
};
use ui_input::InputField;
use workspace::{
    Item, Member, ModalView, MultiWorkspace, Pane, PaneAxis, PaneGroup, SerializableItem,
    SplitDirection, Toast, Workspace, WorkspaceDb, WorkspaceId, item::ItemEvent,
    notifications::NotificationId,
};

use crate::persistence::{
    LAYOUT_VERSION, SerializedAxis, SerializedTerminalGroup, SerializedTile, SerializedTileTree,
    TerminalGroupDb, load_layout,
};
use crate::tile::{HEADER_HEIGHT, new_tile_pane, tile_terminal};

pub(crate) const ORCHESTRATION_SKILL_INSTALL_COMMAND: &str =
    "npx skills add RDG-Labs/rdg --skill rdg-orchestration";

actions!(
    terminal_group,
    [
        /// Opens a new terminal group: a tab containing a tiled terminal workspace.
        New,
        /// Splits the focused tile to the right.
        SplitRight,
        /// Splits the focused tile to the left.
        SplitLeft,
        /// Splits the focused tile upward.
        SplitUp,
        /// Splits the focused tile downward.
        SplitDown,
        /// Moves focus to the tile left of the focused one.
        FocusLeft,
        /// Moves focus to the tile right of the focused one.
        FocusRight,
        /// Moves focus to the tile above the focused one.
        FocusUp,
        /// Moves focus to the tile below the focused one.
        FocusDown,
        /// Moves focus to the next tile in reading order, wrapping at the end.
        FocusNext,
        /// Closes the focused tile.
        CloseTile,
        /// Floats the focused tile above the dimmed grid, or restores it.
        ToggleMagnify,
        /// Exchanges the focused tile with its neighbour to the left.
        SwapLeft,
        /// Exchanges the focused tile with its neighbour to the right.
        SwapRight,
        /// Exchanges the focused tile with the neighbour above.
        SwapUp,
        /// Exchanges the focused tile with the neighbour below.
        SwapDown,
        /// Restores every tile to an equal share of the grid.
        Equalize,
        /// Detaches the terminal group into its own window.
        Detach,
        /// Reattaches a detached terminal group to its source workspace.
        Reattach,
        /// Opens a prompt to launch an arbitrary command in a new tile.
        CustomCommand,
    ]
);

/// Resolved settings for the tiled terminal workspace.
#[derive(Debug, Clone, Copy, settings::RegisterSetting)]
pub struct TerminalWorkspaceSettings {
    pub gap: f32,
    pub corner_radius: f32,
    pub minimum: TileMinimum,
    pub split_guard: SplitGuard,
    pub magnify_size: f32,
    pub max_tiles: usize,
}

impl settings::Settings for TerminalWorkspaceSettings {
    fn from_settings(content: &settings::SettingsContent) -> Self {
        let content = content
            .terminal_workspace
            .as_ref()
            .expect("terminal_workspace defaults are always present");

        Self {
            gap: content.gap.unwrap_or(6.),
            corner_radius: content.corner_radius.unwrap_or(6.),
            minimum: TileMinimum {
                columns: content.min_tile_columns.unwrap_or(30),
                rows: content.min_tile_rows.unwrap_or(8),
            },
            split_guard: match content.split_guard.unwrap_or_default() {
                settings::SplitGuardContent::Adapt => SplitGuard::Adapt,
                settings::SplitGuardContent::Refuse => SplitGuard::Refuse,
                settings::SplitGuardContent::Off => SplitGuard::Off,
            },
            magnify_size: content.magnify_size.unwrap_or(0.92).clamp(0.5, 1.),
            // A group must always be able to hold at least the tile it has.
            max_tiles: content.max_tiles.unwrap_or(32).max(1),
        }
    }
}

pub fn init(cx: &mut App) {
    TerminalWorkspaceSettings::register(cx);
    workspace::register_serializable_item::<TerminalGroup>(cx);

    cx.observe_new(|workspace: &mut Workspace, _window, _cx| {
        workspace.register_action(TerminalGroup::deploy);
        workspace.register_action(forward_focus_next);
        workspace.register_action(forward_close_tile);
        workspace.register_action(forward_equalize);
        workspace.register_action(forward_reattach);
        workspace.register_action(forward_custom_command);
    })
    .detach();
}

/// Routes a terminal-group action to the currently active terminal group tab, so
/// the command palette exposes these actions while an editor or other item is
/// in focus.
fn forward_focus_next(
    workspace: &mut Workspace,
    _: &FocusNext,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    if let Some(group) = active_group(workspace, cx) {
        group.update(cx, |group, cx| group.focus_next(window, cx));
    }
}

fn forward_close_tile(
    workspace: &mut Workspace,
    _: &CloseTile,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    if let Some(group) = active_group(workspace, cx) {
        group.update(cx, |group, cx| {
            let pane = group.active_pane.clone();
            group.close_tile(&pane, window, cx);
        });
    }
}

fn forward_equalize(
    workspace: &mut Workspace,
    _: &Equalize,
    _window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    if let Some(group) = active_group(workspace, cx) {
        group.update(cx, |group, cx| group.equalize(cx));
    }
}

fn forward_reattach(
    workspace: &mut Workspace,
    _: &Reattach,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    if let Some(group) = active_group(workspace, cx) {
        group.update(cx, |group, cx| group.reattach(window, cx));
    }
}

fn forward_custom_command(
    workspace: &mut Workspace,
    _: &CustomCommand,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    if let Some(group) = active_group(workspace, cx) {
        let pane = group.read(cx).active_pane.clone();
        group.update(cx, |group, cx| {
            group.prompt_custom_command(pane, window, cx)
        });
    }
}

fn active_group(workspace: &Workspace, cx: &App) -> Option<Entity<TerminalGroup>> {
    workspace.active_item_as::<TerminalGroup>(cx)
}

pub fn detach_group(
    workspace: &mut Workspace,
    _: &Detach,
    options: WindowOptions,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    let Some(group) = active_group(workspace, cx) else {
        return;
    };
    let Some(source_window) = window.window_handle().downcast::<MultiWorkspace>() else {
        return;
    };

    let source_workspace = workspace.weak_handle();
    let project = workspace.project().clone();
    let app_state = workspace.app_state().clone();
    let source_pane = workspace.active_pane().clone();
    group.update(cx, |group, _cx| {
        group.detached = true;
        group.detached_origin = Some(DetachedOrigin {
            workspace: source_workspace.clone(),
            window: source_window,
        });
    });
    source_pane.update(cx, |pane, cx| {
        pane.remove_item(group.entity_id(), false, false, window, cx);
    });

    let group_for_window = group.clone();
    let source_workspace_for_window = source_workspace.clone();
    let result = cx.open_window(options, move |window, cx| {
        let target_workspace = cx.new(|cx| Workspace::new(None, project, app_state, window, cx));
        let multi_workspace =
            cx.new(|cx| MultiWorkspace::new(target_workspace.clone(), window, cx));

        target_workspace.update(cx, |target_workspace, cx| {
            group_for_window.update(cx, |group, cx| {
                group.rebind_workspace(target_workspace.weak_handle(), None, window, cx);
            });
            target_workspace.add_item_to_active_pane(
                Box::new(group_for_window.clone()),
                None,
                true,
                window,
                cx,
            );
        });

        let detached_window_id = window.window_handle().window_id();
        let target_workspace = target_workspace.downgrade();
        let group_for_persistence = group_for_window.clone();
        let multi_workspace_for_persistence = multi_workspace.clone();
        let database = WorkspaceDb::global(cx);
        let window_id = detached_window_id.as_u64();
        window.spawn(cx, async move |cx| {
            let workspace_id = database.next_id().await?;
            target_workspace.update_in(cx, |workspace, window, cx| {
                workspace.set_database_id(workspace_id);
                group_for_persistence.update(cx, |group, cx| {
                    group.rebind_workspace(workspace.weak_handle(), Some(workspace_id), window, cx);
                });
            })?;
            multi_workspace_for_persistence.update_in(cx, |multi_workspace, _, cx| {
                multi_workspace.serialize(cx);
            })?;
            database
                .set_session_binding(workspace_id, None, Some(window_id))
                .await?;
            anyhow::Ok(())
        })
        .detach_and_log_err(cx);

        let source_workspace = source_workspace_for_window;
        let group = group_for_window.clone();
        let subscription = cx.on_window_closed(move |cx, window_id| {
            if window_id != detached_window_id {
                return;
            }
            if !group.read(cx).detached {
                return;
            }
            let Some(source_workspace) = source_workspace.upgrade() else {
                return;
            };
            if let Err(error) = source_window.update(cx, |_, source_window, cx| {
                source_workspace.update(cx, |source_workspace, cx| {
                    group.update(cx, |group, cx| {
                        group.detached = false;
                        group.detached_origin = None;
                        group.rebind_workspace(
                            source_workspace.weak_handle(),
                            source_workspace.database_id(),
                            source_window,
                            cx,
                        );
                    });
                    source_workspace.add_item_to_active_pane(
                        Box::new(group.clone()),
                        None,
                        true,
                        source_window,
                        cx,
                    );
                    group.update(cx, |group, cx| {
                        group.rebind_workspace(
                            source_workspace.weak_handle(),
                            source_workspace.database_id(),
                            source_window,
                            cx,
                        );
                    });
                });
            }) {
                log::error!("failed to reattach terminal group after window close: {error:#}");
            }
        });
        multi_workspace.update(cx, |multi_workspace, _cx| {
            multi_workspace.add_window_closed_subscription(subscription);
        });
        window.activate_window();
        multi_workspace
    });

    if let Err(error) = result {
        log::error!("failed to detach terminal group: {error:#}");
        group.update(cx, |group, cx| {
            group.detached = false;
            group.detached_origin = None;
            group.rebind_workspace(source_workspace, workspace.database_id(), window, cx);
        });
        workspace.add_item_to_active_pane(Box::new(group), None, true, window, cx);
    }
}

/// A tile being dragged by its header.
#[derive(Clone)]
pub struct DraggedTile {
    pub pane: Entity<Pane>,
    pub group: WeakEntity<TerminalGroup>,
    pub title: SharedString,
}

struct DetachedOrigin {
    workspace: WeakEntity<Workspace>,
    window: WindowHandle<MultiWorkspace>,
}

impl Render for DraggedTile {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        h_flex()
            .h(px(HEADER_HEIGHT))
            .px_2()
            .gap_1p5()
            .rounded_sm()
            .border_1()
            .border_color(cx.theme().colors().border_focused)
            .bg(cx.theme().colors().elevated_surface_background)
            .child(
                ui::Icon::new(ui::IconName::Terminal)
                    .size(ui::IconSize::XSmall)
                    .color(Color::Muted),
            )
            .child(
                ui::Label::new(self.title.clone())
                    .size(ui::LabelSize::Small)
                    .single_line(),
            )
    }
}

fn to_rect(bounds: gpui::Bounds<Pixels>) -> Rect {
    Rect::new(
        bounds.origin.x.into(),
        bounds.origin.y.into(),
        bounds.size.width.into(),
        bounds.size.height.into(),
    )
}

/// The axis a pane sits directly in, if any.
fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

#[cfg(unix)]
fn control_cli_wrapper() -> Option<(String, String)> {
    let (cli_path, rdg_path) = std::env::current_exe()
        .ok()
        .and_then(|path| {
            let parent = path.parent()?;
            Some((parent.join("cli"), parent.join("rdg")))
        })
        .filter(|(cli_path, rdg_path)| cli_path.is_file() && rdg_path.is_file())?;
    let wrapper_directory = std::env::temp_dir().join(format!("rdg-control-{}", std::process::id()));
    if let Err(error) = std::fs::create_dir_all(&wrapper_directory) {
        log::error!("failed to create RDG control wrapper directory: {error:#}");
        return None;
    }
    let wrapper_path = wrapper_directory.join("rdg");
    let wrapper = format!(
        "#!/bin/sh\nexec {} --zed {} \"$@\"\n",
        shell_quote(&cli_path.to_string_lossy()),
        shell_quote(&rdg_path.to_string_lossy()),
    );
    if let Err(error) = std::fs::write(&wrapper_path, wrapper) {
        log::error!("failed to write RDG control wrapper: {error:#}");
        return None;
    }
    use std::os::unix::fs::PermissionsExt as _;
    if let Err(error) = std::fs::set_permissions(
        &wrapper_path,
        std::fs::Permissions::from_mode(0o755),
    ) {
        log::error!("failed to make RDG control wrapper executable: {error:#}");
        return None;
    }
    Some((
        wrapper_directory.to_string_lossy().into_owned(),
        wrapper_path.to_string_lossy().into_owned(),
    ))
}

#[cfg(not(unix))]
fn control_cli_wrapper() -> Option<(String, String)> {
    None
}

fn parent_axis<'a>(member: &'a Member, pane: &Entity<Pane>) -> Option<&'a PaneAxis> {
    let Member::Axis(axis) = member else {
        return None;
    };
    let holds_pane = axis
        .members
        .iter()
        .any(|child| matches!(child, Member::Pane(candidate) if candidate == pane));
    if holds_pane {
        return Some(axis);
    }
    axis.members
        .iter()
        .find_map(|child| parent_axis(child, pane))
}

fn index_in_axis(axis: &PaneAxis, pane: &Entity<Pane>) -> Option<usize> {
    axis.members
        .iter()
        .position(|child| matches!(child, Member::Pane(candidate) if candidate == pane))
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkerInfo {
    pub id: u64,
    pub parent_id: Option<u64>,
    pub title: String,
    pub cwd: Option<String>,
    pub status: String,
    pub summary: Option<String>,
}

#[derive(Clone, Debug)]
pub enum WorkerEvent {
    Spawned {
        worker_id: u64,
        parent_id: Option<u64>,
    },
    Updated {
        worker_id: u64,
        status: String,
        summary: Option<String>,
    },
    Closed {
        worker_id: u64,
    },
}

#[derive(Debug, Clone)]
struct WorkerMetadata {
    parent_id: Option<u64>,
    command: String,
    status: String,
    summary: Option<String>,
}

/// Sibling proportions captured before a split, so they can be restored after.
struct AxisSnapshot {
    flexes: Vec<f32>,
    source_index: usize,
}

pub struct TerminalGroup {
    center: PaneGroup,
    active_pane: Entity<Pane>,
    magnified_pane: Option<WeakEntity<Pane>>,
    workspace: WeakEntity<Workspace>,
    project: Entity<Project>,
    focus_handle: FocusHandle,
    /// Needed because `Item::handle_drop` is handed `&self` and a bare `App`,
    /// with no context to reach this entity through.
    weak_self: WeakEntity<Self>,
    title: Option<SharedString>,
    /// Sizes a tile will have once it is next laid out. A tile created by a
    /// split has no measured bounds until the following frame, and the guard
    /// must not refuse a legitimate split just because paint has not caught up.
    predicted_sizes: HashMap<gpui::EntityId, (f32, f32)>,
    /// Tiles with a shell already on the way. Focusing a tile starts its shell,
    /// and a tile can be focused before an earlier spawn has landed; without
    /// this the tile would get two terminals and immediately split itself.
    spawning: HashSet<gpui::EntityId>,
    worker_metadata: HashMap<u64, WorkerMetadata>,
    auto_start_empty_tiles: bool,
    /// Where an in-flight tile drag would land. Recomputed as the pointer moves
    /// and cleared when the drag ends, however it ends.
    drop_target: Option<(Entity<Pane>, DropZone)>,
    detached: bool,
    detached_origin: Option<DetachedOrigin>,
    /// Brings restored tiles up shortly after the grid is visible. Held so the
    /// work stops if the group is closed while tiles are still starting.
    _deferred_spawns: Option<Task<()>>,
    _subscriptions: Vec<Subscription>,
}

impl TerminalGroup {
    fn deploy(
        workspace: &mut Workspace,
        _: &New,
        window: &mut Window,
        cx: &mut Context<Workspace>,
    ) {
        let group = Self::new(workspace, window, cx);
        workspace.add_item_to_active_pane(Box::new(group.clone()), None, true, window, cx);

    }

    pub fn new(
        workspace: &mut Workspace,
        window: &mut Window,
        cx: &mut Context<Workspace>,
    ) -> Entity<Self> {
        let weak_workspace = workspace.weak_handle();
        let project = workspace.project().clone();
        Self::build(weak_workspace, project, window, cx)
    }

    fn build(
        weak_workspace: WeakEntity<Workspace>,
        project: Entity<Project>,
        window: &mut Window,
        cx: &mut App,
    ) -> Entity<Self> {
        cx.new(|cx| {
            let focus_handle = cx.focus_handle();
            let pane = new_tile_pane(
                weak_workspace.clone(),
                project.clone(),
                cx.entity().downgrade(),
                window,
                cx,
            );

            let subscriptions = vec![
                cx.subscribe_in(&pane, window, Self::handle_pane_event),
                cx.observe(&pane, |_, _, cx| cx.notify()),
            ];

            Self {
                center: PaneGroup::new(pane.clone()),
                active_pane: pane,
                magnified_pane: None,
                workspace: weak_workspace,
                project,
                focus_handle,
                weak_self: cx.entity().downgrade(),
                title: None,
                predicted_sizes: HashMap::default(),
                spawning: HashSet::default(),
                worker_metadata: HashMap::default(),
                auto_start_empty_tiles: false,
                drop_target: None,
                detached: false,
                detached_origin: None,
                _deferred_spawns: None,
                _subscriptions: subscriptions,
            }
        })
    }

    pub fn is_active_pane(&self, pane: &Entity<Pane>) -> bool {
        &self.active_pane == pane
    }

    pub fn is_magnified(&self, pane: &Entity<Pane>) -> bool {
        self.magnified_pane
            .as_ref()
            .and_then(|magnified| magnified.upgrade())
            .is_some_and(|magnified| &magnified == pane)
    }

    pub fn tiles(&self) -> Vec<&Entity<Pane>> {
        self.center.panes()
    }

    /// Spawns a shell into an empty tile.
    ///
    /// Errors propagate to the caller so a failed spawn surfaces in the UI
    /// rather than leaving a silently empty tile.
    fn spawn_terminal_into(
        &mut self,
        pane: Entity<Pane>,
        working_directory: Option<std::path::PathBuf>,
        init_command: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<Result<()>> {
        self.spawning.insert(pane.entity_id());

        // Weak handles across the await point: a group closed while its shell
        // is still starting must not be kept alive by the pending spawn.
        let project = self.project.downgrade();
        let workspace = self.workspace.clone();
        let pane_id = pane.entity_id();
        let pane = pane.downgrade();

        cx.spawn_in(window, async move |this, cx| {
            // Clear the in-flight mark however this task ends, so a failed
            // spawn does not leave the tile permanently unable to retry.
            let _guard = util::defer({
                let this = this.clone();
                let mut cx = cx.clone();
                move || {
                    this.update(&mut cx, |this, _| {
                        this.spawning.remove(&pane_id);
                    })
                    .ok();
                }
            });

            let terminal = project
                .update(cx, |project, cx| {
                    project.create_terminal_shell(working_directory, cx)
                })?
                .await?;

            if let Some(command) = init_command {
                terminal.update(cx, |terminal, cx| {
                    terminal
                        .write_init_command_after_startup(format!("{command}\r").into_bytes(), cx);
                });
            }

            // Read the workspace only once the synchronous update that started
            // this task has finished; an action handler still holds it.
            let workspace_id = workspace
                .read_with(cx, |workspace, _| workspace.database_id())
                .ok()
                .flatten();

            pane.update_in(cx, |pane, window, cx| {
                let terminal_view = cx.new(|cx| {
                    TerminalView::new(
                        terminal,
                        workspace.clone(),
                        workspace_id,
                        project.clone(),
                        window,
                        cx,
                    )
                });
                pane.add_item(Box::new(terminal_view), true, true, None, window, cx);
            })?;

            Ok(())
        })
    }

    /// Cell geometry, taken from any live terminal in the group.
    ///
    /// Font settings are shared, so every tile agrees; asking the whole group
    /// means a tile that has not painted yet still gets real numbers.
    fn cell_metrics(&self, cx: &App) -> Option<(f32, f32)> {
        self.center.panes().into_iter().find_map(|pane| {
            let terminal_view = tile_terminal(pane.read(cx), cx)?;
            let bounds = terminal_view
                .read(cx)
                .terminal()
                .read(cx)
                .last_content()
                .terminal_bounds;
            let cell_width: f32 = bounds.cell_width().into();
            let line_height: f32 = bounds.line_height().into();
            (cell_width > 0. && line_height > 0.).then_some((cell_width, line_height))
        })
    }

    /// The tile's box in pixels.
    ///
    /// A terminal knows its own painted size, which is the only measurement
    /// available to a single-tile group: `bounding_box_for_pane` returns `None`
    /// when the tree root is a bare pane.
    fn tile_size(&self, pane: &Entity<Pane>, cx: &App) -> Option<(f32, f32)> {
        let painted = tile_terminal(pane.read(cx), cx).and_then(|terminal_view| {
            let bounds = terminal_view
                .read(cx)
                .terminal()
                .read(cx)
                .last_content()
                .terminal_bounds;
            let width: f32 = bounds.width().into();
            let height: f32 = bounds.height().into();
            (width > 0. && height > 0.).then_some((width, height + HEADER_HEIGHT))
        });

        painted
            .or_else(|| {
                self.center
                    .bounding_box_for_pane(pane)
                    .map(|bounds| (bounds.size.width.into(), bounds.size.height.into()))
            })
            .or_else(|| self.predicted_sizes.get(&pane.entity_id()).copied())
    }

    /// Geometry the guard decides against, or `None` when the group has never
    /// been painted and there is nothing to measure.
    fn metrics_for(&self, pane: &Entity<Pane>, cx: &App) -> Option<TileMetrics> {
        let (width, height) = self.tile_size(pane, cx)?;
        let (cell_width, line_height) = self.cell_metrics(cx)?;

        Some(TileMetrics {
            width,
            height,
            cell_width,
            line_height,
            gap: TerminalWorkspaceSettings::get_global(cx).gap,
            header_height: HEADER_HEIGHT,
        })
    }

    /// Gives the new tile half of the split tile's space and leaves every other
    /// sibling untouched.
    ///
    /// `PaneAxis::insert_pane` re-equalizes the whole axis, which is right for
    /// editor panes but destroys a deliberately sized grid: adding one scratch
    /// shell should not resize the five terminals beside it. Wave has the same
    /// flaw, and it is visible the moment you split a tuned column.
    fn restore_sibling_sizes(
        &mut self,
        source: &Entity<Pane>,
        new_pane: &Entity<Pane>,
        snapshot: Option<AxisSnapshot>,
    ) {
        let Some(snapshot) = snapshot else {
            return;
        };
        // A perpendicular split nests the pair in a fresh two-member axis, which
        // is already an even halving. Only a same-axis insert needs repair.
        let Some(axis) = parent_axis(&self.center.root, new_pane) else {
            return;
        };
        let (Some(source_index), Some(new_index)) =
            (index_in_axis(axis, source), index_in_axis(axis, new_pane))
        else {
            return;
        };

        let previous_len = snapshot.flexes.len();
        if axis.members.len() != previous_len + 1
            || snapshot.source_index >= previous_len
            || previous_len == 0
        {
            return;
        }

        // Flexes are normalized so they sum to the member count. Rescaling by
        // (n + 1) / n preserves every sibling's share of the axis.
        let scale = (previous_len + 1) as f32 / previous_len as f32;
        let source_share = snapshot.flexes[snapshot.source_index] * scale / 2.;

        let mut rebuilt = Vec::with_capacity(axis.members.len());
        let mut previous = snapshot.flexes.iter().enumerate();
        for index in 0..axis.members.len() {
            if index == source_index || index == new_index {
                rebuilt.push(source_share);
                continue;
            }
            let next = previous
                .find(|(previous_index, _)| *previous_index != snapshot.source_index)
                .map(|(_, flex)| *flex * scale)
                .unwrap_or(1.);
            rebuilt.push(next);
        }

        *axis.flexes.lock() = rebuilt;
    }

    /// Records what the two halves of a split will measure, so a split issued
    /// before the next paint is still judged against real geometry.
    fn predict_split_sizes(
        &mut self,
        source: &Entity<Pane>,
        new_pane: &Entity<Pane>,
        metrics: TileMetrics,
        direction: SplitDirection,
    ) {
        let halves = match direction {
            SplitDirection::Left | SplitDirection::Right => {
                let half = (metrics.width - metrics.gap) / 2.;
                ((half, metrics.height), (half, metrics.height))
            }
            SplitDirection::Up | SplitDirection::Down => {
                let half = (metrics.height - metrics.gap) / 2.;
                ((metrics.width, half), (metrics.width, half))
            }
        };
        self.predicted_sizes.insert(source.entity_id(), halves.0);
        self.predicted_sizes.insert(new_pane.entity_id(), halves.1);
    }

    fn split(&mut self, direction: SplitDirection, window: &mut Window, cx: &mut Context<Self>) {
        let source = self.active_pane.clone();
        self.split_pane(&source, direction, window, cx);
    }

    /// Splits `source`, subject to the usability guard.
    ///
    /// A refusal creates nothing and spawns nothing: a terminal the user cannot
    /// read is worse than no terminal at all.
    fn split_pane(
        &mut self,
        source: &Entity<Pane>,
        direction: SplitDirection,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.split_pane_with_command(source, direction, None, None, window, cx);
    }

    fn split_pane_with_command(
        &mut self,
        source: &Entity<Pane>,
        direction: SplitDirection,
        init_command: Option<String>,
        parent_id: Option<u64>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Entity<Pane>> {
        let source = source.clone();

        // The cap is the backstop that holds even when nothing can be measured,
        // so an unpainted group can never be split without bound.
        let settings = *TerminalWorkspaceSettings::get_global(cx);
        if self.center.panes().len() >= settings.max_tiles {
            self.report_no_room(cx);
            return None;
        }

        // An unpainted group has no geometry to judge. Refusing there would
        // block a legitimate first split; the next one is measured and governed.
        let metrics = self.metrics_for(&source, cx);
        let direction = match metrics {
            Some(metrics) => {
                match resolve_split(metrics, direction, settings.minimum, settings.split_guard) {
                    SplitOutcome::Split(direction) => direction,
                    SplitOutcome::Refused => {
                        self.report_no_room(cx);
                        return None;
                    }
                }
            }
            None => direction,
        };

        // Magnify hides the grid, so a split while magnified would land
        // invisibly. Restore first, then split.
        self.magnified_pane = None;

        // Inherit the split tile's directory. Deliberately no workspace lookup
        // here: `split_pane` can run inside a pane subscription, and reading the
        // workspace from a context that already holds it panics. `None` lets the
        // project pick its own default.
        let working_directory = tile_terminal(source.read(cx), cx).and_then(|terminal_view| {
            terminal_view
                .read(cx)
                .terminal()
                .read(cx)
                .working_directory()
        });

        let snapshot = parent_axis(&self.center.root, &source).and_then(|axis| {
            index_in_axis(axis, &source).map(|source_index| AxisSnapshot {
                flexes: axis.flexes.lock().clone(),
                source_index,
            })
        });

        let new_pane = self.new_tile(window, cx);
        self.center.split(&source, &new_pane, direction, cx);
        self.restore_sibling_sizes(&source, &new_pane, snapshot);
        cx.emit(ItemEvent::UpdateTab);
        if let Some(metrics) = metrics {
            self.predict_split_sizes(&source, &new_pane, metrics, direction);
        }
        self.set_active_pane(&new_pane, window, cx);
        let init_command = init_command.map(|command| {
            self.worker_init_command(&new_pane, command, parent_id)
        });
        self.spawn_terminal_into(new_pane.clone(), working_directory, init_command, window, cx)
            .detach_and_log_err(cx);
        cx.notify();
        Some(new_pane)
    }

    fn report_no_room(&self, cx: &mut Context<Self>) {
        self.workspace
            .update(cx, |workspace, cx| {
                workspace.show_toast(
                    Toast::new(
                        NotificationId::unique::<TerminalGroup>(),
                        "Not enough room for another terminal. Magnify, close a tile, or open a new group.",
                    ),
                    cx,
                );
            })
            .ok();
    }

    fn set_active_pane(
        &mut self,
        pane: &Entity<Pane>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.active_pane = pane.clone();
        window.focus(&pane.focus_handle(cx), cx);
        cx.notify();
    }

    /// Moves focus to the neighboring tile in `direction`.
    ///
    /// Navigation is geometric rather than tree-ordered, so it matches what the
    /// user sees. At an edge this is a no-op: focus never wraps and never
    /// leaves the group.
    fn focus_in_direction(
        &mut self,
        direction: SplitDirection,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let target = self
            .center
            .find_pane_in_direction(&self.active_pane, direction, cx)
            .cloned();
        if let Some(target) = target {
            self.set_active_pane(&target, window, cx);
        }
    }

    fn focus_next(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let panes = self.center.panes().into_iter().cloned().collect::<Vec<_>>();
        let Some(index) = panes.iter().position(|pane| pane == &self.active_pane) else {
            return;
        };
        let next = panes[(index + 1) % panes.len()].clone();
        self.set_active_pane(&next, window, cx);
    }

    /// Exchanges the focused tile with its neighbour, the keyboard equivalent
    /// of dragging one tile onto another's centre.
    fn swap_in_direction(
        &mut self,
        direction: SplitDirection,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(target) = self
            .center
            .find_pane_in_direction(&self.active_pane, direction, cx)
            .cloned()
        else {
            return;
        };
        let active = self.active_pane.clone();
        self.center.swap(&active, &target, cx);
        self.set_active_pane(&active, window, cx);
        cx.emit(ItemEvent::UpdateTab);
        cx.notify();
    }

    fn equalize(&mut self, cx: &mut Context<Self>) {
        self.center.reset_pane_sizes(cx);
        cx.emit(ItemEvent::UpdateTab);
        cx.notify();
    }

    /// Adds a tile beside `pane`. Backs the `+` button in the tile header,
    /// which is the only mouse-driven way to grow a grid — the tab bar's own
    /// `+` belongs to the workspace pane and creates items outside the group.
    pub fn split_tile(&mut self, pane: &Entity<Pane>, window: &mut Window, cx: &mut Context<Self>) {
        self.split_pane(pane, SplitDirection::Right, window, cx);
    }

    fn start_terminal_in_pane(
        &mut self,
        pane: Entity<Pane>,
        command: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if pane.read(cx).items_len() > 0 || self.spawning.contains(&pane.entity_id()) {
            return;
        }
        let init_command = command.clone().map(|command| {
            let worker_id = pane.entity_id().as_u64();
            let shell_command = command.clone();
            self.worker_metadata.insert(
                worker_id,
                WorkerMetadata {
                    parent_id: None,
                    command,
                    status: "starting".to_string(),
                    summary: None,
                },
            );
            cx.emit(WorkerEvent::Spawned {
                worker_id,
                parent_id: None,
            });
            self.worker_init_command(&pane, shell_command, None)
        });
        let working_directory = self.project_root(cx);
        self.spawn_terminal_into(pane, working_directory, init_command, window, cx)
            .detach_and_log_err(cx);
    }

    fn prompt_custom_command(
        &self,
        pane: Entity<Pane>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(workspace) = self.workspace.upgrade() else {
            return;
        };
        let group = self.weak_self.clone();
        window.defer(cx, move |window, cx| {
            workspace.update(cx, |workspace, cx| {
                workspace.toggle_modal(window, cx, |window, cx| {
                    CustomCommandModal::new(group, pane, window, cx)
                });
            });
        });
    }

    fn rebind_workspace(
        &mut self,
        workspace: WeakEntity<Workspace>,
        workspace_id: Option<WorkspaceId>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.workspace = workspace.clone();
        for pane in self.center.panes() {
            pane.update(cx, |pane, _cx| pane.rebind_workspace(workspace.clone()));
            if let Some(terminal) = tile_terminal(pane.read(cx), cx) {
                terminal.update(cx, |terminal, cx| {
                    terminal.rebind_workspace(workspace.clone(), workspace_id, window, cx);
                });
            }
        }
        cx.emit(ItemEvent::UpdateTab);
    }

    fn reattach(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(origin) = self.detached_origin.take() else {
            return;
        };
        let Some(group) = self.weak_self.upgrade() else {
            return;
        };
        let target_pane = self.active_pane.clone();
        let target_workspace = self.workspace.clone();
        let target_workspace_id = target_workspace
            .read_with(cx, |workspace, _| workspace.database_id())
            .ok()
            .flatten();

        window.defer(cx, move |window, cx| {
            target_pane.update(cx, |pane, cx| {
                pane.remove_item(group.entity_id(), false, false, window, cx);
            });

            let source_workspace = origin.workspace.clone();
            let result = origin.window.update(cx, |_, source_window, cx| {
                let Some(source_workspace) = source_workspace.upgrade() else {
                    return Err(anyhow::anyhow!("source workspace no longer exists"));
                };
                source_workspace.update(cx, |source_workspace, cx| {
                    group.update(cx, |group, cx| {
                        group.detached = false;
                        group.rebind_workspace(
                            source_workspace.weak_handle(),
                            source_workspace.database_id(),
                            source_window,
                            cx,
                        );
                    });
                    source_workspace.add_item_to_active_pane(
                        Box::new(group.clone()),
                        None,
                        true,
                        source_window,
                        cx,
                    );
                    group.update(cx, |group, cx| {
                        group.rebind_workspace(
                            source_workspace.weak_handle(),
                            source_workspace.database_id(),
                            source_window,
                            cx,
                        );
                    });
                });
                window.remove_window();
                Ok(())
            });

            if let Err(error) = result {
                log::error!("failed to reattach terminal group: {error:#}");
                if let Some(target_workspace) = target_workspace.upgrade() {
                    target_workspace.update(cx, |target_workspace, cx| {
                        group.update(cx, |group, cx| {
                            group.detached = true;
                            group.detached_origin = Some(origin);
                            group.rebind_workspace(
                                target_workspace.weak_handle(),
                                target_workspace_id,
                                window,
                                cx,
                            );
                        });
                        target_workspace.add_item_to_active_pane(
                            Box::new(group),
                            None,
                            true,
                            window,
                            cx,
                        );
                    });
                }
            }
        });
    }

    pub fn spawn_agent_beside(
        &mut self,
        pane: &Entity<Pane>,
        command: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let worker_command = command.clone();
        if let Some(pane) = self.split_pane_with_command(
            pane,
            SplitDirection::Right,
            Some(command),
            None,
            window,
            cx,
        ) {
            let worker_id = pane.entity_id().as_u64();
            self.worker_metadata.insert(
                worker_id,
                WorkerMetadata {
                    parent_id: None,
                    command: worker_command,
                    status: "starting".to_string(),
                    summary: None,
                },
            );
            cx.emit(WorkerEvent::Spawned {
                worker_id,
                parent_id: None,
            });
        }
    }

    fn worker_init_command(
        &self,
        pane: &Entity<Pane>,
        command: String,
        parent_id: Option<u64>,
    ) -> String {
        let group_id = self.weak_self.entity_id().as_u64();
        let worker_id = pane.entity_id().as_u64();
        #[cfg(windows)]
        let prefix = format!(
            "set RDG_GROUP_ID={group_id} && set RDG_WORKER_ID={worker_id}{} && ",
            parent_id.map_or(String::new(), |id| format!(" && set RDG_PARENT_WORKER_ID={id}")),
        );
        #[cfg(not(windows))]
        let prefix = {
            let (control_directory, control_command) =
                control_cli_wrapper().unwrap_or_default();
            let path_prefix = if control_directory.is_empty() {
                String::new()
            } else {
                format!("export PATH={}:$PATH; ", shell_quote(&control_directory))
            };
            format!(
                "{path_prefix}export RDG_GROUP_ID={group_id} RDG_WORKER_ID={worker_id}{} RDG_CONTROL_COMMAND={}; rdg() {{ \"$RDG_CONTROL_COMMAND\" \"$@\"; }}; if [ -n \"$BASH_VERSION\" ]; then export -f rdg; fi; ",
                parent_id.map_or(String::new(), |id| format!(" RDG_PARENT_WORKER_ID={id}")),
                shell_quote(if control_command.is_empty() {
                    "rdg"
                } else {
                    &control_command
                }),
            )
        };
        format!("{prefix}{command}")
    }

    pub fn control_list(&self, cx: &App) -> Vec<WorkerInfo> {
        self.center
            .panes()
            .into_iter()
            .map(|pane| {
                let id = pane.entity_id().as_u64();
                let metadata = self.worker_metadata.get(&id);
                let (title, cwd) = tile_terminal(pane.read(cx), cx)
                    .map(|terminal_view| {
                        let terminal = terminal_view.read(cx).terminal().read(cx);
                        (
                            terminal
                                .foreground_process_command_name()
                                .unwrap_or_else(|| terminal.title(true)),
                            terminal
                                .working_directory()
                                .map(|path| path.to_string_lossy().into_owned()),
                        )
                    })
                    .unwrap_or_else(|| ("Terminal".to_string(), None));
                WorkerInfo {
                    id,
                    parent_id: metadata.and_then(|metadata| metadata.parent_id),
                    title,
                    cwd,
                    status: metadata
                        .map(|metadata| metadata.status.clone())
                        .unwrap_or_else(|| "unmanaged".to_string()),
                    summary: metadata.and_then(|metadata| metadata.summary.clone()),
                }
            })
            .collect()
    }

    pub fn control_spawn(
        &mut self,
        source_id: Option<u64>,
        parent_id: Option<u64>,
        command: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<u64> {
        let worker_command = command.clone();
        let source = source_id
            .and_then(|id| {
                self.center
                    .panes()
                    .into_iter()
                    .find(|pane| pane.entity_id().as_u64() == id)
                    .cloned()
            })
            .unwrap_or_else(|| self.active_pane.clone());
        let pane = self.split_pane_with_command(
            &source,
            SplitDirection::Right,
            Some(command),
            parent_id,
            window,
            cx,
        )?;
        let id = pane.entity_id().as_u64();
        self.worker_metadata.insert(
            id,
            WorkerMetadata {
                parent_id,
                command: worker_command,
                status: "starting".to_string(),
                summary: None,
            },
        );
        cx.emit(WorkerEvent::Spawned {
            worker_id: id,
            parent_id,
        });
        Some(id)
    }

    pub fn control_send(&self, worker_id: u64, text: &str, cx: &mut App) -> bool {
        let Some(pane) = self
            .center
            .panes()
            .into_iter()
            .find(|pane| pane.entity_id().as_u64() == worker_id)
        else {
            return false;
        };
        let Some(terminal_view) = tile_terminal(pane.read(cx), cx) else {
            return false;
        };
        let terminal = terminal_view.read(cx).terminal().clone();
        terminal.update(cx, |terminal, _| terminal.input(text.as_bytes().to_vec()));
        true
    }

    pub fn control_close(
        &mut self,
        worker_id: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(pane) = self
            .center
            .panes()
            .into_iter()
            .find(|pane| pane.entity_id().as_u64() == worker_id)
            .cloned()
        else {
            return false;
        };
        self.worker_metadata.remove(&worker_id);
        cx.emit(WorkerEvent::Closed { worker_id });
        self.close_tile(&pane, window, cx);
        cx.notify();
        true
    }

    pub fn control_close_subtree(
        &mut self,
        root_worker_id: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let mut pending = vec![root_worker_id];
        let mut worker_ids = HashSet::<u64>::default();
        while let Some(worker_id) = pending.pop() {
            if !worker_ids.insert(worker_id) {
                continue;
            }
            pending.extend(
                self.worker_metadata
                    .iter()
                    .filter(|(_, metadata)| metadata.parent_id == Some(worker_id))
                    .map(|(worker_id, _)| *worker_id),
            );
        }

        let mut closed = false;
        for worker_id in worker_ids {
            closed |= self.control_close(worker_id, window, cx);
        }
        closed
    }

    pub fn control_restart(
        &mut self,
        worker_id: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(metadata) = self.worker_metadata.remove(&worker_id) else {
            return false;
        };
        let Some(pane) = self
            .center
            .panes()
            .into_iter()
            .find(|pane| pane.entity_id().as_u64() == worker_id)
            .cloned()
        else {
            return false;
        };
        let parent_id = metadata.parent_id;
        let command = metadata.command;
        let close_task = pane.update(cx, |pane, cx| {
            pane.close_all_items(&Default::default(), window, cx)
        });
        cx.emit(WorkerEvent::Closed { worker_id });
        let group = self.weak_self.clone();
        window
            .spawn(cx, async move |cx| {
                close_task.await?;
                group
                    .update_in(cx, |group, window, cx| {
                        group.control_spawn(parent_id, parent_id, command, window, cx);
                    })
                    .map(|_| ())
            })
            .detach_and_log_err(cx);
        cx.notify();
        true
    }

    pub fn control_report(
        &mut self,
        worker_id: u64,
        status: String,
        summary: Option<String>,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(metadata) = self.worker_metadata.get_mut(&worker_id) else {
            return false;
        };
        metadata.status = status.clone();
        metadata.summary = summary.clone();
        cx.emit(WorkerEvent::Updated {
            worker_id,
            status,
            summary,
        });
        cx.notify();
        true
    }

    /// Places a terminal that arrived from outside into a tile of its own.
    fn adopt_terminal(
        &mut self,
        item: Box<dyn workspace::ItemHandle>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Land it beside whichever tile the pointer was over, falling back to
        // the focused one.
        let target = self
            .drop_target
            .take()
            .map(|(pane, _)| pane)
            .filter(|pane| self.center.panes().contains(&pane))
            .unwrap_or_else(|| self.active_pane.clone());

        // Room was confirmed before the drop was accepted; the guard is
        // consulted again here only to pick the better axis.
        let direction = match self.metrics_for(&target, cx) {
            Some(metrics) => {
                let settings = *TerminalWorkspaceSettings::get_global(cx);
                match resolve_split(
                    metrics,
                    SplitDirection::Right,
                    settings.minimum,
                    settings.split_guard,
                ) {
                    SplitOutcome::Split(adapted) => adapted,
                    SplitOutcome::Refused => SplitDirection::Right,
                }
            }
            None => SplitDirection::Right,
        };

        let new_pane = self.new_tile(window, cx);
        new_pane.update(cx, |new_pane, cx| {
            new_pane.add_item(item, true, true, None, window, cx);
        });
        self.center.split(&target, &new_pane, direction, cx);
        self.set_active_pane(&new_pane, window, cx);
        cx.emit(ItemEvent::UpdateTab);
        cx.notify();
    }

    pub fn toggle_magnify(
        &mut self,
        pane: &Entity<Pane>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.is_magnified(pane) {
            self.magnified_pane = None;
        } else {
            self.magnified_pane = Some(pane.downgrade());
        }
        self.set_active_pane(pane, window, cx);
        cx.emit(ItemEvent::UpdateTab);
    }

    pub fn close_tile(&mut self, pane: &Entity<Pane>, window: &mut Window, cx: &mut Context<Self>) {
        pane.update(cx, |pane, cx| {
            pane.close_all_items(&Default::default(), window, cx)
                .detach_and_log_err(cx);
        });
    }

    /// Removes a tile from the tree, collapsing its axis and moving focus.
    ///
    /// Closing the last tile closes the group tab itself, which is what a user
    /// closing their final terminal expects.
    fn remove_tile(
        &mut self,
        pane: &Entity<Pane>,
        focus_on_pane: Option<Entity<Pane>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.center.panes().len() == 1 {
            cx.emit(ItemEvent::CloseItem);
            return;
        }

        if self.is_magnified(pane) {
            self.magnified_pane = None;
        }

        self.predicted_sizes.remove(&pane.entity_id());
        self.spawning.remove(&pane.entity_id());
        let worker_id = pane.entity_id().as_u64();
        if self.worker_metadata.remove(&worker_id).is_some() {
            cx.emit(WorkerEvent::Closed { worker_id });
        }
        match self.center.remove(pane, cx) {
            Ok(_) => {
                let next = focus_on_pane
                    .filter(|candidate| self.center.panes().contains(&candidate))
                    .unwrap_or_else(|| self.center.first_pane());
                self.set_active_pane(&next, window, cx);
            }
            Err(error) => {
                log::error!("failed to remove terminal tile: {error:#}");
            }
        }
        cx.emit(ItemEvent::UpdateTab);
        cx.notify();
    }

    /// Invariant TG-1: a tile holds exactly one terminal.
    ///
    /// Anything that lands a second item in a tile — a task spawn, a reopened
    /// item, a programmatic add — is moved into a tile of its own rather than
    /// growing a tab strip inside the grid.
    fn enforce_single_terminal(
        &mut self,
        pane: &Entity<Pane>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if pane.read(cx).items_len() <= 1 {
            return;
        }

        let Some(surplus) = pane.update(cx, |pane, cx| pane.take_active_item(window, cx)) else {
            return;
        };

        let new_pane = self.new_tile(window, cx);
        new_pane.update(cx, |new_pane, cx| {
            new_pane.add_item(surplus, true, true, None, window, cx);
        });
        self.center
            .split(pane, &new_pane, SplitDirection::Right, cx);
        self.set_active_pane(&new_pane, window, cx);
        cx.emit(ItemEvent::UpdateTab);
        cx.notify();
    }

    /// Creates a tile pane and subscribes to it. Every tile must go through
    /// here, or its removal and split events are never observed.
    fn new_tile(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Entity<Pane> {
        let pane = new_tile_pane(
            self.workspace.clone(),
            self.project.clone(),
            cx.entity().downgrade(),
            window,
            cx,
        );
        self._subscriptions
            .push(cx.subscribe_in(&pane, window, Self::handle_pane_event));
        pane
    }

    fn handle_pane_event(
        &mut self,
        pane: &Entity<Pane>,
        event: &workspace::pane::Event,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            workspace::pane::Event::Focus => {
                self.active_pane = pane.clone();
                // Magnification follows focus rather than being dropped by it.
                // The magnified tile covers the grid, so moving focus to a tile
                // the user cannot see would be incoherent; instead the magnified
                // view moves with them, and shift-escape is the way out.
                if self.magnified_pane.is_some() && !self.is_magnified(pane) {
                    self.magnified_pane = Some(pane.downgrade());
                }
                if self.auto_start_empty_tiles {
                    self.spawn_if_empty(pane, window, cx);
                }
                cx.notify();
            }
            workspace::pane::Event::Remove { focus_on_pane } => {
                self.remove_tile(pane, focus_on_pane.clone(), window, cx);
            }
            // Zed already binds cmd-d and ctrl-alt-<arrow> to pane splits in
            // terminal context. Intercepting the event here retargets those
            // keys at the tile tree without touching the keymap, and routes
            // them through the guard.
            &workspace::pane::Event::Split { direction, .. } => {
                self.split_pane(&pane.clone(), direction, window, cx);
            }
            workspace::pane::Event::AddItem { item } => {
                if let Some(workspace) = self.workspace.upgrade() {
                    workspace.update(cx, |workspace, cx| {
                        item.added_to_pane(workspace, pane.clone(), window, cx)
                    });
                }
                self.enforce_single_terminal(pane, window, cx);
            }
            workspace::pane::Event::ChangeItemTitle => cx.notify(),
            _ => {}
        }
    }
}

impl Focusable for TerminalGroup {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.active_pane.focus_handle(cx)
    }
}

impl EventEmitter<ItemEvent> for TerminalGroup {}
impl EventEmitter<WorkerEvent> for TerminalGroup {}

impl TerminalGroup {
    pub(crate) fn worker_status(&self, worker_id: u64) -> Option<String> {
        self.worker_metadata
            .get(&worker_id)
            .map(|metadata| metadata.status.clone())
    }

    pub(crate) fn control_focus(
        &mut self,
        worker_id: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(pane) = self
            .center
            .panes()
            .into_iter()
            .find(|pane| pane.entity_id().as_u64() == worker_id)
            .cloned()
        else {
            return false;
        };
        self.set_active_pane(&pane, window, cx);
        true
    }

    pub(crate) fn control_tree(&self, cx: &App) -> Vec<(WorkerInfo, usize)> {
        let workers = self.control_list(cx);
        let mut worker_by_id = workers
            .into_iter()
            .map(|worker| (worker.id, worker))
            .collect::<HashMap<_, _>>();
        let worker_ids = worker_by_id.keys().copied().collect::<Vec<_>>();
        let mut children = HashMap::<u64, Vec<u64>>::default();
        let mut roots = Vec::new();
        for worker_id in worker_ids {
            let Some(worker) = worker_by_id.get(&worker_id) else {
                continue;
            };
            if let Some(parent_id) = worker.parent_id.filter(|id| worker_by_id.contains_key(id)) {
                children.entry(parent_id).or_default().push(worker_id);
            } else {
                roots.push(worker_id);
            }
        }

        let mut tree = Vec::new();
        let mut pending = roots.into_iter().rev().map(|id| (id, 0)).collect::<Vec<_>>();
        while let Some((worker_id, depth)) = pending.pop() {
            let Some(worker) = worker_by_id.remove(&worker_id) else {
                continue;
            };
            tree.push((worker, depth));
            if let Some(children) = children.get(&worker_id) {
                pending.extend(children.iter().rev().map(|id| (*id, depth + 1)));
            }
        }
        tree
    }
}

impl Item for TerminalGroup {
    type Event = ItemEvent;

    /// Takes a terminal dragged onto the grid and gives it a tile of its own.
    ///
    /// `Pane::handle_tab_drop` offers every drop to its active item before
    /// filing it as a sibling tab, which is the seam that lets a group accept
    /// terminals without any change to the pane itself.
    fn handle_drop(
        &self,
        _active_pane: &Pane,
        dropped: &dyn std::any::Any,
        window: &mut Window,
        cx: &mut App,
    ) -> bool {
        let Some(tab) = dropped.downcast_ref::<workspace::DraggedTab>() else {
            return false;
        };

        // Only terminals, so the one-terminal-per-tile invariant holds (TG-1).
        if tab.item.downcast::<TerminalView>().is_none() {
            return false;
        }

        // A tile already in this group is an internal rearrange, which the
        // tile drag handles with its own drop zones.
        if self.center.panes().contains(&&tab.pane) {
            return false;
        }

        // A terminal arriving from outside *adds* a tile, unlike an internal
        // rearrange, so it answers to the guard exactly as a split does.
        // Declining returns false, and the pane files it as a sibling tab —
        // the terminal is never lost, it simply does not join a full grid.
        let target = self.active_pane.clone();
        if let Some(metrics) = self.metrics_for(&target, cx) {
            let settings = *TerminalWorkspaceSettings::get_global(cx);
            if resolve_split(
                metrics,
                SplitDirection::Right,
                settings.minimum,
                settings.split_guard,
            ) == SplitOutcome::Refused
            {
                return false;
            }
        }

        let group = self.weak_self.clone();
        let source = tab.pane.clone();
        let item_id = tab.item.item_id();

        // Deferred because this runs inside the update of both the source pane
        // and this group; moving the item has to touch both again.
        window.defer(cx, move |window, cx| {
            let taken = source.update(cx, |source, cx| {
                let index = source.items().position(|item| item.item_id() == item_id)?;
                source.activate_item(index, false, false, window, cx);
                source.take_active_item(window, cx)
            });

            let Some(item) = taken else {
                return;
            };

            group
                .update(cx, |group, cx| {
                    group.adopt_terminal(item, window, cx);
                })
                .ok();
        });

        true
    }

    fn tab_extra_context_menu_actions(
        &self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Vec<(SharedString, Box<dyn gpui::Action>)> {
        if self.detached {
            vec![("Reattach Group".into(), Box::new(Reattach))]
        } else {
            vec![("Detach Group to New Window".into(), Box::new(Detach))]
        }
    }

    fn tab_content_text(&self, _detail: usize, _cx: &App) -> SharedString {
        let title = self.title.clone().unwrap_or_else(|| {
            let count = self.center.panes().len();
            if count <= 1 {
                SharedString::from("Terminal")
            } else {
                SharedString::from(format!("Terminal ({count})"))
            }
        });
        let active_workers = self
            .worker_metadata
            .values()
            .filter(|metadata| matches!(metadata.status.as_str(), "starting" | "working"))
            .count();
        if active_workers == 0 {
            title
        } else {
            format!("{title} · {active_workers} active").into()
        }
    }

    fn tab_icon(&self, _window: &Window, _cx: &App) -> Option<ui::Icon> {
        Some(ui::Icon::new(ui::IconName::Terminal))
    }

    fn to_item_events(event: &Self::Event, f: &mut dyn FnMut(ItemEvent)) {
        f(*event)
    }
}

struct CustomCommandModal {
    group: WeakEntity<TerminalGroup>,
    pane: Entity<Pane>,
    command: Entity<InputField>,
}

impl CustomCommandModal {
    fn new(
        group: WeakEntity<TerminalGroup>,
        pane: Entity<Pane>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let command = cx.new(|cx| InputField::new(window, cx, "Command to run"));
        Self {
            group,
            pane,
            command,
        }
    }

    fn dismiss(&mut self, cx: &mut Context<Self>) {
        cx.emit(DismissEvent);
    }

    fn confirm(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let command = self.command.read(cx).text(cx).trim().to_owned();
        if command.is_empty() {
            self.command.update(cx, |command, cx| {
                command.set_error(Some("Enter a command to run"), cx);
            });
            return;
        }

        let group = self.group.clone();
        let pane = self.pane.clone();
        self.dismiss(cx);
        window.defer(cx, move |window, cx| {
            if let Some(group) = group.upgrade() {
                group.update(cx, |group, cx| {
                    if pane.read(cx).items_len() == 0 {
                        group.start_terminal_in_pane(pane, Some(command.clone()), window, cx);
                    } else {
                        group.spawn_agent_beside(&pane, command, window, cx);
                    }
                });
            }
        });
    }
}

impl Focusable for CustomCommandModal {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.command.focus_handle(cx)
    }
}

impl EventEmitter<DismissEvent> for CustomCommandModal {}
impl ModalView for CustomCommandModal {}

impl Render for CustomCommandModal {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        AlertModal::new("custom-command-modal")
            .width(rems(36.))
            .key_context("CustomCommandModal")
            .track_focus(&self.focus_handle(cx))
            .on_action(cx.listener(|this, _: &menu::Confirm, window, cx| {
                this.confirm(window, cx);
            }))
            .on_action(cx.listener(|this, _: &menu::Cancel, _window, cx| {
                this.dismiss(cx);
            }))
            .header(Label::new("Launch Custom Command"))
            .child(
                v_flex()
                    .p_3()
                    .gap_2()
                    .child(Label::new("Run any shell command in a new terminal tile.").color(Color::Muted))
                    .child(self.command.clone()),
            )
            .footer(
                h_flex()
                    .px_3()
                    .pb_3()
                    .gap_1()
                    .justify_end()
                    .child(
                        Button::new("cancel", "Cancel")
                            .on_click(cx.listener(|this, _, _, cx| this.dismiss(cx))),
                    )
                    .child(
                        Button::new("run", "Run")
                            .style(ButtonStyle::Filled)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.confirm(window, cx);
                            })),
                    ),
            )
            .into_any_element()
    }
}

/// Flattens the live tree into its stored shape.
fn serialize_tree(member: &Member) -> SerializedTileTree {
    match member {
        Member::Pane(_) => SerializedTileTree::Tile(SerializedTile::default()),
        Member::Axis(axis) => SerializedTileTree::Axis {
            axis: SerializedAxis::from(axis.axis),
            flexes: axis.flexes.lock().clone(),
            children: axis.members.iter().map(serialize_tree).collect(),
        },
    }
}

impl SerializableItem for TerminalGroup {
    fn serialized_item_kind() -> &'static str {
        "TerminalGroup"
    }

    fn cleanup(
        workspace_id: WorkspaceId,
        alive_items: Vec<workspace::ItemId>,
        _window: &mut Window,
        cx: &mut App,
    ) -> Task<Result<()>> {
        let db = TerminalGroupDb::global(cx);
        workspace::delete_unloaded_items(alive_items, workspace_id, "terminal_groups", &db, cx)
    }

    fn serialize(
        &mut self,
        workspace: &mut Workspace,
        item_id: workspace::ItemId,
        _closing: bool,
        cx: &mut Context<Self>,
    ) -> Option<Task<Result<()>>> {
        let workspace_id = workspace.database_id()?;

        let tiles = self.center.panes().into_iter().cloned().collect::<Vec<_>>();
        let index_of = |target: &Entity<Pane>| tiles.iter().position(|tile| tile == target);

        let layout = SerializedTerminalGroup {
            version: LAYOUT_VERSION,
            root: serialize_tree(&self.center.root),
            focused_tile: index_of(&self.active_pane),
            magnified_tile: self
                .magnified_pane
                .as_ref()
                .and_then(|pane| pane.upgrade())
                .and_then(|pane| index_of(&pane)),
            title: self.title.as_ref().map(|title| title.to_string()),
        };

        let db = TerminalGroupDb::global(cx);
        let encoded = match serde_json::to_string(&layout) {
            Ok(encoded) => encoded,
            Err(error) => {
                log::error!("failed to encode terminal group layout: {error:#}");
                return None;
            }
        };

        Some(
            cx.background_spawn(
                async move { db.save_layout(item_id, workspace_id, encoded).await },
            ),
        )
    }

    fn should_serialize(&self, event: &Self::Event) -> bool {
        // The tab label carries the tile count, so every structural change
        // already emits this.
        matches!(event, ItemEvent::UpdateTab)
    }

    fn deserialize(
        project: Entity<Project>,
        workspace: WeakEntity<Workspace>,
        workspace_id: WorkspaceId,
        item_id: workspace::ItemId,
        window: &mut Window,
        cx: &mut App,
    ) -> Task<Result<Entity<Self>>> {
        let max_tiles = TerminalWorkspaceSettings::get_global(cx).max_tiles;
        let layout = load_layout(item_id, workspace_id, max_tiles, cx);
        let group = Self::build(workspace, project, window, cx);

        if let Some(layout) = layout {
            group.update(cx, |group, cx| {
                group.title = layout.title.clone().map(SharedString::from);
                group.restore_layout(&layout, window, cx);
            });
        }

        Task::ready(Ok(group))
    }
}

impl TerminalGroup {
    /// Rebuilds the tile tree from a stored layout.
    ///
    /// Tiles come back empty: shells are spawned when a tile is first focused,
    /// so restoring a twelve-tile grid costs one process rather than twelve.
    fn restore_layout(
        &mut self,
        layout: &SerializedTerminalGroup,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut tiles = Vec::new();
        let root = self.build_tree(&layout.root, &mut tiles, window, cx);
        if tiles.is_empty() {
            return;
        }

        self.center = PaneGroup::with_root(root);

        let magnified = layout
            .magnified_tile
            .and_then(|index| tiles.get(index))
            .cloned();

        // Magnification follows focus, so a magnified tile is the focused tile.
        let focused = magnified
            .clone()
            .or_else(|| {
                layout
                    .focused_tile
                    .and_then(|index| tiles.get(index))
                    .cloned()
            })
            .unwrap_or_else(|| tiles[0].clone());

        self.active_pane = focused.clone();
        self.magnified_pane = magnified.map(|pane| pane.downgrade());
        self.auto_start_empty_tiles = true;

        // The tile the user lands on starts immediately so the grid is usable
        // at once; the rest follow shortly after (§8.4). A tile with no terminal
        // renders no header, so leaving them empty indefinitely would show a
        // wall of blank boxes.
        let root = self.project_root(cx);
        self.spawn_terminal_into(focused, root.clone(), None, window, cx)
            .detach_and_log_err(cx);
        self._deferred_spawns = Some(self.spawn_remaining_tiles(root, window, cx));
        cx.notify();
    }

    /// Starts the shells for tiles still waiting, one at a time.
    ///
    /// Sequential rather than concurrent: a restored grid should not fork a
    /// dozen processes at once on a cold app.
    fn spawn_remaining_tiles(
        &mut self,
        root: Option<std::path::PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<()> {
        cx.spawn_in(window, async move |this, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(500))
                .await;
            let mut failed = HashSet::new();

            loop {
                let next = this
                    .update(cx, |this, cx| {
                        this.center
                            .panes()
                            .into_iter()
                            .find(|pane| {
                                pane.read(cx).items_len() == 0
                                    && !this.spawning.contains(&pane.entity_id())
                                    && !failed.contains(&pane.entity_id())
                            })
                            .cloned()
                    })
                    .ok()
                    .flatten();

                let Some(pane) = next else {
                    break;
                };
                let pane_id = pane.entity_id();

                let task = this.update_in(cx, |this, window, cx| {
                    this.spawn_terminal_into(pane, root.clone(), None, window, cx)
                });
                match task {
                    Ok(task) => {
                        if let Err(error) = task.await {
                            log::error!("failed to start a restored terminal: {error:#}");
                            failed.insert(pane_id);
                        }
                    }
                    Err(error) => {
                        log::error!("failed to schedule a restored terminal: {error:#}");
                        failed.insert(pane_id);
                    }
                }
            }
        })
    }

    fn build_tree(
        &mut self,
        node: &SerializedTileTree,
        tiles: &mut Vec<Entity<Pane>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Member {
        match node {
            SerializedTileTree::Tile(_) => {
                let pane = self.new_tile(window, cx);
                tiles.push(pane.clone());
                Member::Pane(pane)
            }
            SerializedTileTree::Axis {
                axis,
                flexes,
                children,
            } => {
                let members = children
                    .iter()
                    .map(|child| self.build_tree(child, tiles, window, cx))
                    .collect::<Vec<_>>();
                Member::Axis(PaneAxis::load(
                    (*axis).into(),
                    members,
                    Some(flexes.clone()),
                ))
            }
        }
    }

    /// The project's first visible worktree, used as the working directory for
    /// tiles that have none of their own.
    ///
    /// Read from the project rather than the workspace on purpose: this runs
    /// from pane subscriptions, where the workspace may already be borrowed.
    fn project_root(&self, cx: &App) -> Option<std::path::PathBuf> {
        self.project
            .read(cx)
            .visible_worktrees(cx)
            .next()
            .map(|worktree| worktree.read(cx).abs_path().to_path_buf())
    }

    /// The grid's own bounds, taken as the union of its tiles.
    ///
    /// A single-tile group has no bounding box by construction, and also has
    /// nothing to rearrange, so `None` correctly disables dragging there.
    fn group_bounds(&self) -> Option<gpui::Bounds<Pixels>> {
        self.center
            .panes()
            .into_iter()
            .filter_map(|pane| self.center.bounding_box_for_pane(pane))
            .reduce(|accumulated, bounds| accumulated.union(&bounds))
    }

    /// Where a drop at `position` would land, in window coordinates.
    fn resolve_drop_target(&self, position: Point<Pixels>) -> Option<(Entity<Pane>, DropZone)> {
        let group = self.group_bounds()?;
        let pane = self.center.pane_at_pixel_position(position)?.clone();
        let tile = self.center.bounding_box_for_pane(&pane)?;

        let zone = zone_at(
            position.x.into(),
            position.y.into(),
            to_rect(tile),
            to_rect(group),
        );
        Some((pane, zone))
    }

    /// Carries out a drop.
    ///
    /// Each zone maps onto an operation `PaneGroup` already provides, so the
    /// tree manipulation itself is code Zed exercises elsewhere.
    fn apply_drop(
        &mut self,
        dragged: Entity<Pane>,
        target: Entity<Pane>,
        zone: DropZone,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if dragged == target && !matches!(zone, DropZone::Outer(_)) {
            // Dropping a tile onto its own centre or edge is a no-op, not a
            // self-swap.
            return;
        }

        match zone {
            DropZone::Swap => {
                // Sizes stay with the positions rather than travelling with the
                // tiles, which is what "swap" looks like to the eye.
                self.center.swap(&dragged, &target, cx);
            }
            DropZone::Edge(direction) => {
                // A move is not a split. The tile already exists, the source
                // vacates its old space, and the count is unchanged — so the
                // guard advises here, it does not veto. Refusing would leave a
                // dense grid impossible to rearrange, which is a worse failure
                // than a tile the user deliberately made small and can drag
                // back. The guard still picks the better axis when the
                // requested one is cramped.
                let direction = match self.metrics_for(&target, cx) {
                    Some(metrics) => {
                        let settings = *TerminalWorkspaceSettings::get_global(cx);
                        match resolve_split(
                            metrics,
                            direction,
                            settings.minimum,
                            settings.split_guard,
                        ) {
                            SplitOutcome::Split(adapted) => adapted,
                            SplitOutcome::Refused => direction,
                        }
                    }
                    None => direction,
                };

                match self.center.remove(&dragged, cx) {
                    Ok(_) => self.center.split(&target, &dragged, direction, cx),
                    Err(error) => {
                        log::error!("failed to move terminal tile: {error:#}");
                        return;
                    }
                }
            }
            DropZone::Outer(direction) => {
                if let Err(error) = self.center.move_to_border(&dragged, direction, cx) {
                    log::error!("failed to promote terminal tile: {error:#}");
                    return;
                }
            }
        }

        self.set_active_pane(&dragged, window, cx);
        cx.emit(ItemEvent::UpdateTab);
        cx.notify();
    }

    fn spawn_if_empty(&mut self, pane: &Entity<Pane>, window: &mut Window, cx: &mut Context<Self>) {
        if pane.read(cx).items_len() > 0 || self.spawning.contains(&pane.entity_id()) {
            return;
        }
        let root = self.project_root(cx);
        self.spawn_terminal_into(pane.clone(), root, None, window, cx)
            .detach_and_log_err(cx);
    }
}

impl Render for TerminalGroup {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(workspace) = self.workspace.upgrade() else {
            return div().size_full();
        };

        let magnified = self.magnified_pane.as_ref().and_then(|pane| pane.upgrade());

        // A drag that ended anywhere — dropped, cancelled with escape, or
        // released outside the group — leaves no event on this element, so the
        // stale target is cleared here rather than in a handler.
        if !cx.has_active_drag() && self.drop_target.is_some() {
            self.drop_target = None;
        }

        let drop_preview = self.drop_target.as_ref().and_then(|(pane, zone)| {
            let group = to_rect(self.group_bounds()?);
            let tile = to_rect(self.center.bounding_box_for_pane(pane)?);
            let rect = preview_rect(*zone, tile, group);
            // Window coordinates into coordinates local to this group.
            Some(Rect::new(
                rect.x - group.x,
                rect.y - group.y,
                rect.width,
                rect.height,
            ))
        });

        // The magnified tile is rendered by the overlay, so the grid must skip
        // it. `PaneGroup::render` takes the pane to omit as `zoomed`; without
        // this the same pane is rendered twice in one frame.
        let zoomed: Option<gpui::AnyWeakView> =
            magnified.as_ref().map(|pane| pane.downgrade().into());

        let follower_states = HashMap::default();
        let grid = workspace.update(cx, |workspace, cx| {
            let weak_workspace = workspace.weak_handle();
            let render_cx = workspace::PaneRenderContext {
                follower_states: &follower_states,
                active_pane: &self.active_pane,
                app_state: workspace.app_state(),
                project: workspace.project(),
                workspace: &weak_workspace,
            };
            self.center
                .render(zoomed.as_ref(), None, &render_cx, window, cx)
                .into_any_element()
        });

        let empty_launcher = (self.center.panes().len() == 1
            && self.center.first_pane().read(cx).items_len() == 0
            && self.spawning.is_empty())
        .then(|| render_empty_launcher(self.center.first_pane(), window, cx));
        let worker_tree = self.control_tree(cx);
        let group = cx.entity().downgrade();
        let mission_control = PopoverMenu::new("mission-control")
            .trigger_with_tooltip(
                IconButton::new("mission-control", IconName::ListTree)
                    .shape(IconButtonShape::Square)
                    .icon_size(IconSize::XSmall),
                Tooltip::text("Mission Control"),
            )
            .menu(move |window, cx| {
                let group = group.upgrade()?;
                let worker_tree = worker_tree.clone();
                Some(ContextMenu::build(window, cx, move |menu, _window: &mut Window, _| {
                    let mut menu = menu.label("Mission Control");
                    for (worker, depth) in &worker_tree {
                        let marker = match worker.status.as_str() {
                            "completed" => "✓",
                            "failed" => "✕",
                            "waiting" => "○",
                            "starting" | "working" => "●",
                            _ => "·",
                        };
                        let label = format!(
                            "{}{} {}  {}",
                            "  ".repeat(*depth),
                            marker,
                            worker.title,
                            worker.status
                        );
                        let worker_id = worker.id;
                        let group_for_focus = group.clone();
                        let group_for_restart = group.clone();
                        let group_for_close = group.clone();
                        let group_for_subtree = group.clone();
                        menu = menu.submenu(label, move |menu, window, _| {
                            let group_for_focus = group_for_focus.clone();
                            let group_for_restart = group_for_restart.clone();
                            let group_for_close = group_for_close.clone();
                            let group_for_subtree = group_for_subtree.clone();
                            let menu = menu.entry(
                                "Focus Worker",
                                None,
                                window.handler_for(&group_for_focus, move |group, window, cx| {
                                    group.control_focus(worker_id, window, cx);
                                }),
                            );
                            let menu = menu.entry(
                                "Restart Worker",
                                None,
                                window.handler_for(&group_for_restart, move |group, window, cx| {
                                    group.control_restart(worker_id, window, cx);
                                }),
                            );
                            let menu = menu.entry(
                                "Close Worker",
                                None,
                                window.handler_for(&group_for_close, move |group, window, cx| {
                                    group.control_close(worker_id, window, cx);
                                }),
                            );
                            menu.entry(
                                "Close Worker Subtree",
                                None,
                                window.handler_for(&group_for_subtree, move |group, window, cx| {
                                    group.control_close_subtree(worker_id, window, cx);
                                }),
                            )
                        });
                    }
                    menu
                }))
            });

        div()
            .size_full()
            .relative()
            .track_focus(&self.focus_handle)
            .key_context("TerminalGroup")
            .bg(cx.theme().colors().terminal_background)
            .on_action(cx.listener(|this, _: &SplitRight, window, cx| {
                this.split(SplitDirection::Right, window, cx)
            }))
            .on_action(cx.listener(|this, _: &SplitLeft, window, cx| {
                this.split(SplitDirection::Left, window, cx)
            }))
            .on_action(cx.listener(|this, _: &SplitUp, window, cx| {
                this.split(SplitDirection::Up, window, cx)
            }))
            .on_action(cx.listener(|this, _: &SplitDown, window, cx| {
                this.split(SplitDirection::Down, window, cx)
            }))
            .on_action(cx.listener(|this, _: &FocusLeft, window, cx| {
                this.focus_in_direction(SplitDirection::Left, window, cx)
            }))
            .on_action(cx.listener(|this, _: &FocusRight, window, cx| {
                this.focus_in_direction(SplitDirection::Right, window, cx)
            }))
            .on_action(cx.listener(|this, _: &FocusUp, window, cx| {
                this.focus_in_direction(SplitDirection::Up, window, cx)
            }))
            .on_action(cx.listener(|this, _: &FocusDown, window, cx| {
                this.focus_in_direction(SplitDirection::Down, window, cx)
            }))
            .on_action(cx.listener(|this, _: &FocusNext, window, cx| this.focus_next(window, cx)))
            .on_action(cx.listener(|this, _: &CloseTile, window, cx| {
                let pane = this.active_pane.clone();
                this.close_tile(&pane, window, cx)
            }))
            .on_action(cx.listener(|this, _: &ToggleMagnify, window, cx| {
                let pane = this.active_pane.clone();
                this.toggle_magnify(&pane, window, cx)
            }))
            .on_action(cx.listener(|this, _: &SwapLeft, window, cx| {
                this.swap_in_direction(SplitDirection::Left, window, cx)
            }))
            .on_action(cx.listener(|this, _: &SwapRight, window, cx| {
                this.swap_in_direction(SplitDirection::Right, window, cx)
            }))
            .on_action(cx.listener(|this, _: &SwapUp, window, cx| {
                this.swap_in_direction(SplitDirection::Up, window, cx)
            }))
            .on_action(cx.listener(|this, _: &SwapDown, window, cx| {
                this.swap_in_direction(SplitDirection::Down, window, cx)
            }))
            .on_action(cx.listener(|this, _: &Equalize, _window, cx| this.equalize(cx)))
            .on_action(cx.listener(|this, _: &Reattach, window, cx| {
                this.reattach(window, cx);
            }))
            // Escape cancels an in-flight drag. The terminal binds bare escape
            // to SendKeystroke, so without this a drag could only be ended by
            // dropping it somewhere.
            .on_action(cx.listener(|this, _: &menu::Cancel, window, cx| {
                if cx.stop_active_drag(window) {
                    this.drop_target = None;
                    cx.notify();
                } else {
                    cx.propagate();
                }
            }))
            .on_drag_move::<DraggedTile>(cx.listener(
                |this, event: &DragMoveEvent<DraggedTile>, _window, cx| {
                    let target = this.resolve_drop_target(event.event.position);
                    if this.drop_target != target {
                        this.drop_target = target;
                        cx.notify();
                    }
                },
            ))
            .on_drop::<DraggedTile>(cx.listener(|this, dragged: &DraggedTile, window, cx| {
                let Some((target, zone)) = this.drop_target.take() else {
                    return;
                };
                this.apply_drop(dragged.pane.clone(), target, zone, window, cx);
            }))
            .child(
                div()
                    .size_full()
                    .p(px(TerminalWorkspaceSettings::get_global(cx).gap / 2.))
                    .child(grid),
            )
            .child(
                div()
                    .absolute()
                    .top_2()
                    .right_2()
                    .child(mission_control),
            )
            .children(empty_launcher)
            .children(drop_preview.map(|rect| {
                // A filled region rather than an insertion line: it shows the
                // shape the tile will actually occupy.
                div()
                    .absolute()
                    .left(px(rect.x))
                    .top(px(rect.y))
                    .w(px(rect.width))
                    .h(px(rect.height))
                    .rounded_sm()
                    .border_2()
                    .border_color(cx.theme().colors().border_focused)
                    .bg(cx.theme().colors().drop_target_background)
            }))
            .children(magnified.map(|pane| {
                render_magnified(
                    pane,
                    TerminalWorkspaceSettings::get_global(cx).magnify_size,
                    window,
                    cx,
                )
            }))
    }
}

fn render_empty_launcher(
    pane: Entity<Pane>,
    _window: &mut Window,
    cx: &mut Context<TerminalGroup>,
) -> AnyElement {
    let mut actions = h_flex()
        .gap_2()
        .justify_center()
        .flex_wrap()
        .child(
            Button::new("empty-new-terminal", "New Terminal")
                .size(ButtonSize::Compact)
                .style(ButtonStyle::Filled)
                .on_click(cx.listener({
                    let pane = pane.clone();
                    move |group, _, window, cx| {
                        group.start_terminal_in_pane(pane.clone(), None, window, cx);
                    }
                })),
        )
        .child(
            Button::new("empty-custom-command", "Custom Command…")
                .size(ButtonSize::Compact)
                .on_click(cx.listener({
                    let pane = pane.clone();
                    move |group, _, window, cx| {
                        group.prompt_custom_command(pane.clone(), window, cx);
                    }
                })),
        );

    for agent in installed_agents() {
        let command = agent.command.to_owned();
        actions = actions.child(
            Button::new(format!("empty-agent-{}", agent.command), agent.name)
                .size(ButtonSize::Compact)
                .on_click(cx.listener({
                    let pane = pane.clone();
                    let command = command.clone();
                    move |group, _, window, cx| {
                        group.start_terminal_in_pane(
                            pane.clone(),
                            Some(command.clone()),
                            window,
                            cx,
                        );
                    }
                })),
        );
    }

    v_flex()
        .absolute()
        .inset_0()
        .items_center()
        .justify_center()
        .gap_3()
        .bg(cx.theme().colors().terminal_background)
        .child(Label::new("Start a terminal group").size(LabelSize::Large))
        .child(
            Label::new("Launch a shell or external coding CLI in this tile.")
                .color(Color::Muted),
        )
        .child(actions)
        .into_any_element()
}

/// The magnified tile floats above a dimmed grid rather than replacing it, so
/// the user keeps their sense of place. The backdrop is a flat dim, never a
/// blur: a full-viewport blur is a per-frame GPU cost on a surface already
/// compositing every terminal behind it.
fn render_magnified(
    pane: Entity<Pane>,
    size: f32,
    _window: &mut Window,
    cx: &mut Context<TerminalGroup>,
) -> AnyElement {
    div()
        .absolute()
        .inset_0()
        .flex()
        .items_center()
        .justify_center()
        .bg(cx.theme().colors().terminal_background.opacity(0.6))
        .on_mouse_down(
            gpui::MouseButton::Left,
            cx.listener(|this, _, window, cx| {
                let pane = this.active_pane.clone();
                this.toggle_magnify(&pane, window, cx);
            }),
        )
        .child(
            div()
                .w(gpui::relative(size))
                .h(gpui::relative(size))
                .rounded_md()
                .overflow_hidden()
                .border_1()
                .border_color(cx.theme().colors().border_focused)
                .bg(cx.theme().colors().terminal_background)
                .child(pane),
        )
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{Entity, TestAppContext, VisualTestContext};
    use project::{FakeFs, Project};
    use settings::SettingsStore;
    use workspace::MultiWorkspace;

    fn init_test(cx: &mut TestAppContext) {
        cx.update(|cx| {
            let store = SettingsStore::test(cx);
            cx.set_global(store);
            theme_settings::init(theme::LoadThemes::JustBase, cx);
            editor::init(cx);
            terminal_view::init(cx);
            crate::init(cx);
        });
    }

    async fn init_group(
        cx: &mut TestAppContext,
    ) -> (gpui::WindowHandle<MultiWorkspace>, Entity<TerminalGroup>) {
        let fs = FakeFs::new(cx.executor());
        let project = Project::test(fs, [], cx).await;
        let window_handle =
            cx.add_window(|window, cx| MultiWorkspace::test_new(project, window, cx));

        let group = window_handle
            .update(cx, |multi_workspace, window, cx| {
                multi_workspace.workspace().update(cx, |workspace, cx| {
                    let group = TerminalGroup::new(workspace, window, cx);
                    workspace.add_item_to_active_pane(
                        Box::new(group.clone()),
                        None,
                        true,
                        window,
                        cx,
                    );
                    group.update(cx, |group, cx| {
                        let tile = group.active_pane.clone();
                        group
                            .spawn_terminal_into(tile, None, None, window, cx)
                            .detach();
                    });
                    group
                })
            })
            .expect("failed to create terminal group");

        cx.run_until_parked();
        (window_handle, group)
    }

    #[gpui::test]
    async fn test_new_group_starts_with_a_single_tile(cx: &mut TestAppContext) {
        cx.executor().allow_parking();
        init_test(cx);

        let (_window, group) = init_group(cx).await;

        group.read_with(cx, |group, _| {
            assert_eq!(group.tiles().len(), 1);
            assert!(group.magnified_pane.is_none());
        });
    }

    #[gpui::test]
    async fn test_split_adds_a_tile_and_focuses_it(cx: &mut TestAppContext) {
        cx.executor().allow_parking();
        init_test(cx);

        let (window, group) = init_group(cx).await;

        window
            .update(cx, |_, window, cx| {
                group.update(cx, |group, cx| {
                    group.split(SplitDirection::Right, window, cx);
                });
            })
            .expect("failed to split");
        cx.run_until_parked();

        group.read_with(cx, |group, _| {
            assert_eq!(group.tiles().len(), 2);
            // The new tile takes focus, matching Wave.
            assert_eq!(&group.active_pane, group.tiles()[1]);
        });
    }

    #[gpui::test]
    async fn test_closing_a_tile_collapses_the_axis(cx: &mut TestAppContext) {
        cx.executor().allow_parking();
        init_test(cx);

        let (window, group) = init_group(cx).await;

        window
            .update(cx, |_, window, cx| {
                group.update(cx, |group, cx| {
                    group.split(SplitDirection::Right, window, cx);
                    group.split(SplitDirection::Down, window, cx);
                });
            })
            .expect("failed to split");
        cx.run_until_parked();

        group.read_with(cx, |group, _| assert_eq!(group.tiles().len(), 3));

        let doomed = group.read_with(cx, |group, _| group.active_pane.clone());
        window
            .update(cx, |_, window, cx| {
                group.update(cx, |group, cx| {
                    group.close_tile(&doomed, window, cx);
                });
            })
            .expect("failed to close tile");
        cx.run_until_parked();

        group.read_with(cx, |group, _| {
            assert_eq!(group.tiles().len(), 2);
            assert!(!group.tiles().contains(&&doomed));
        });
    }

    #[gpui::test]
    async fn test_magnify_is_non_destructive(cx: &mut TestAppContext) {
        cx.executor().allow_parking();
        init_test(cx);

        let (window, group) = init_group(cx).await;

        window
            .update(cx, |_, window, cx| {
                group.update(cx, |group, cx| {
                    group.split(SplitDirection::Right, window, cx);
                });
            })
            .expect("failed to split");
        cx.run_until_parked();

        let target = group.read_with(cx, |group, _| group.active_pane.clone());

        window
            .update(cx, |_, window, cx| {
                group.update(cx, |group, cx| {
                    group.toggle_magnify(&target, window, cx);
                });
            })
            .expect("failed to magnify");

        group.read_with(cx, |group, _| {
            assert!(group.is_magnified(&target));
            // The tree is untouched: magnify is a presentation state.
            assert_eq!(group.tiles().len(), 2);
        });

        window
            .update(cx, |_, window, cx| {
                group.update(cx, |group, cx| {
                    group.toggle_magnify(&target, window, cx);
                });
            })
            .expect("failed to restore");

        group.read_with(cx, |group, _| {
            assert!(!group.is_magnified(&target));
            assert_eq!(group.tiles().len(), 2);
        });
    }

    /// The magnified tile covers the grid, so moving focus must move the
    /// magnified view with it rather than leaving the user looking at a tile
    /// that no longer has focus.
    #[gpui::test]
    async fn test_magnification_follows_focus(cx: &mut TestAppContext) {
        cx.executor().allow_parking();
        init_test(cx);

        let (window, group) = init_group(cx).await;
        let mut visual = VisualTestContext::from_window(window.into(), cx);
        visual.simulate_resize(gpui::size(gpui::px(3200.), gpui::px(1200.)));
        visual.run_until_parked();

        visual.update(|window, cx| {
            group.update(cx, |group, cx| {
                group.split(SplitDirection::Right, window, cx);
            });
        });
        visual.run_until_parked();

        let (first, second) = group.read_with(&mut visual, |group, _| {
            (group.tiles()[0].clone(), group.tiles()[1].clone())
        });

        visual.update(|window, cx| {
            group.update(cx, |group, cx| {
                group.toggle_magnify(&second, window, cx);
                group.set_active_pane(&first, window, cx);
            });
        });
        visual.run_until_parked();

        group.read_with(&mut visual, |group, _| {
            assert!(
                group.is_magnified(&first),
                "magnification should follow focus to the newly focused tile"
            );
            assert!(!group.is_magnified(&second));
        });
    }

    #[gpui::test]
    async fn test_splitting_while_magnified_restores_first(cx: &mut TestAppContext) {
        cx.executor().allow_parking();
        init_test(cx);

        let (window, group) = init_group(cx).await;
        let target = group.read_with(cx, |group, _| group.active_pane.clone());

        window
            .update(cx, |_, window, cx| {
                group.update(cx, |group, cx| {
                    group.toggle_magnify(&target, window, cx);
                    group.split(SplitDirection::Right, window, cx);
                });
            })
            .expect("failed to split while magnified");
        cx.run_until_parked();

        group.read_with(cx, |group, _| {
            assert!(group.magnified_pane.is_none());
            assert_eq!(group.tiles().len(), 2);
        });
    }

    #[gpui::test]
    async fn test_a_second_item_in_a_tile_moves_to_its_own_tile(cx: &mut TestAppContext) {
        cx.executor().allow_parking();
        init_test(cx);

        let (window, group) = init_group(cx).await;
        let tile = group.read_with(cx, |group, _| group.active_pane.clone());

        // Invariant TG-1: adding a second terminal to a tile must not grow a
        // tab strip inside the grid.
        window
            .update(cx, |_, window, cx| {
                group.update(cx, |group, cx| {
                    group
                        .spawn_terminal_into(tile.clone(), None, None, window, cx)
                        .detach();
                });
            })
            .expect("failed to add a second terminal");
        cx.run_until_parked();

        group.read_with(cx, |group, cx| {
            assert_eq!(
                group.tiles().len(),
                2,
                "the surplus terminal needs its own tile"
            );
            for tile in group.tiles() {
                assert_eq!(
                    tile.read(cx).items_len(),
                    1,
                    "a tile must hold exactly one terminal"
                );
            }
        });
    }

    #[gpui::test]
    async fn test_runaway_splitting_terminates(cx: &mut TestAppContext) {
        cx.executor().allow_parking();
        init_test(cx);

        let (window, group) = init_group(cx).await;

        // Forty split requests against a finite window. Wave answers every one
        // of them and leaves a row of unreadable slivers; the guard must stop
        // well before that, and the cap bounds it even if measurement fails.
        window
            .update(cx, |_, window, cx| {
                group.update(cx, |group, cx| {
                    for _ in 0..40 {
                        group.split(SplitDirection::Right, window, cx);
                    }
                });
            })
            .expect("failed to split");
        cx.run_until_parked();

        group.read_with(cx, |group, cx| {
            let tiles = group.tiles().len();
            assert!(tiles > 1, "the first splits should succeed, got {tiles}");
            assert!(
                tiles < 40,
                "forty splits must not yield forty tiles, got {tiles}"
            );
            assert!(
                tiles <= TerminalWorkspaceSettings::get_global(cx).max_tiles,
                "the cap must hold, got {tiles}"
            );
        });
    }

    /// Ship gate 4, at the integration level: after real layout, repeated splits
    /// must never leave a tile too small to use. This is the failure reproduced
    /// in Wave Terminal and the reason the guard exists.
    #[gpui::test]
    async fn test_rendered_splits_never_produce_an_unusable_tile(cx: &mut TestAppContext) {
        cx.executor().allow_parking();
        init_test(cx);

        let (window, group) = init_group(cx).await;
        let mut visual = VisualTestContext::from_window(window.into(), cx);
        visual.simulate_resize(gpui::size(gpui::px(1400.), gpui::px(900.)));
        visual.run_until_parked();

        for _ in 0..10 {
            visual.update(|window, cx| {
                group.update(cx, |group, cx| {
                    group.split(SplitDirection::Right, window, cx);
                });
            });
            visual.run_until_parked();
        }

        group.read_with(&mut visual, |group, cx| {
            let minimum = TileMinimum::default();
            for tile in group.tiles() {
                // Tiles that have painted must clear the usability floor.
                let Some(terminal_view) = tile_terminal(tile.read(cx), cx) else {
                    continue;
                };
                let bounds = terminal_view
                    .read(cx)
                    .terminal()
                    .read(cx)
                    .last_content()
                    .terminal_bounds;
                let width: f32 = bounds.width().into();
                if width <= 0. {
                    continue;
                }
                assert!(
                    bounds.num_columns() >= minimum.columns,
                    "guard admitted a {}-column tile, floor is {}",
                    bounds.num_columns(),
                    minimum.columns
                );
            }
        });
    }

    /// A split must take space from the tile being split, not from every tile
    /// on the axis. Wave re-equalizes, which destroys a tuned grid the moment
    /// you add a scratch shell.
    #[gpui::test]
    async fn test_split_preserves_sibling_proportions(cx: &mut TestAppContext) {
        cx.executor().allow_parking();
        init_test(cx);

        let (window, group) = init_group(cx).await;
        let mut visual = VisualTestContext::from_window(window.into(), cx);
        // Wide enough that the guard cannot adapt the axis out from under the
        // assertion; this test is about sizing, not about the guard.
        visual.simulate_resize(gpui::size(gpui::px(3200.), gpui::px(1200.)));
        visual.run_until_parked();

        visual.update(|window, cx| {
            group.update(cx, |group, cx| {
                group.split(SplitDirection::Right, window, cx);
            });
        });
        visual.run_until_parked();

        // Give the row deliberately uneven proportions.
        group.read_with(&mut visual, |group, _| {
            let Member::Axis(axis) = &group.center.root else {
                panic!("expected a row of tiles");
            };
            assert_eq!(axis.members.len(), 2);
            *axis.flexes.lock() = vec![1.4, 0.6];
        });

        visual.update(|window, cx| {
            group.update(cx, |group, cx| {
                group.split(SplitDirection::Right, window, cx);
            });
        });
        visual.run_until_parked();

        group.read_with(&mut visual, |group, _| {
            let Member::Axis(axis) = &group.center.root else {
                panic!("expected a row of tiles");
            };
            let flexes = axis.flexes.lock().clone();
            assert_eq!(flexes.len(), 3, "the split should add one member");

            // Flexes are normalized to sum to the member count.
            let total: f32 = flexes.iter().sum();
            assert!(
                (total - 3.).abs() < 0.001,
                "flexes must sum to the member count, got {total}"
            );

            // The untouched tile keeps its share of the axis: 1.4 of 2 is 70%,
            // which is 2.1 of 3.
            assert!(
                (flexes[0] - 2.1).abs() < 0.001,
                "untouched sibling was resized, flex is {}",
                flexes[0]
            );

            // The split tile's share was halved between it and the new tile.
            assert!(
                (flexes[1] - flexes[2]).abs() < 0.001,
                "the split should halve evenly, got {} and {}",
                flexes[1],
                flexes[2]
            );
            assert!(
                (flexes[1] + flexes[2] - 0.9).abs() < 0.001,
                "the pair should hold the split tile's original share, got {}",
                flexes[1] + flexes[2]
            );
        });
    }

    /// Layout persistence is shape-only, so a serialize/restore round trip must
    /// reproduce the tree exactly: same nesting, same proportions, same focus.
    #[gpui::test]
    async fn test_layout_round_trips_through_serialization(cx: &mut TestAppContext) {
        cx.executor().allow_parking();
        init_test(cx);

        let (window, group) = init_group(cx).await;
        let mut visual = VisualTestContext::from_window(window.into(), cx);
        visual.simulate_resize(gpui::size(gpui::px(3200.), gpui::px(1600.)));
        visual.run_until_parked();

        // A row containing a nested column: the shape that actually exercises
        // recursion in both directions.
        for direction in [SplitDirection::Right, SplitDirection::Down] {
            visual.update(|window, cx| {
                group.update(cx, |group, cx| {
                    group.split(direction, window, cx);
                });
            });
            visual.run_until_parked();
        }

        let (encoded, original_tiles) = group.read_with(&mut visual, |group, _| {
            (serialize_tree(&group.center.root), group.tiles().len())
        });
        assert_eq!(original_tiles, 3);
        assert!(encoded.is_well_formed());
        assert_eq!(encoded.tile_count(), 3);

        let layout = SerializedTerminalGroup {
            version: LAYOUT_VERSION,
            root: encoded.clone(),
            focused_tile: Some(2),
            magnified_tile: Some(2),
            title: Some("services".into()),
        };

        // Restore into a second group and compare the shapes.
        let (_window2, restored) = init_group(cx).await;
        let mut visual2 = VisualTestContext::from_window(_window2.into(), cx);
        visual2.update(|window, cx| {
            restored.update(cx, |restored, cx| {
                restored.restore_layout(&layout, window, cx);
            });
        });
        visual2.run_until_parked();

        restored.read_with(&mut visual2, |restored, _| {
            assert_eq!(restored.tiles().len(), 3);
            assert_eq!(
                serialize_tree(&restored.center.root),
                encoded,
                "the restored tree must match the stored shape"
            );
            assert_eq!(&restored.active_pane, restored.tiles()[2]);
            assert!(restored.is_magnified(restored.tiles()[2]));
        });
    }

    /// Restore starts exactly one shell — the tile the user lands on — so a
    /// large grid comes back without forking a process per tile. The rest
    /// follow on focus, or on the deferred pass 500ms later.
    #[gpui::test]
    async fn test_restore_starts_only_the_focused_tile(cx: &mut TestAppContext) {
        cx.executor().allow_parking();
        init_test(cx);

        let (window, group) = init_group(cx).await;
        let mut visual = VisualTestContext::from_window(window.into(), cx);
        visual.simulate_resize(gpui::size(gpui::px(3200.), gpui::px(1600.)));
        visual.run_until_parked();

        let layout = SerializedTerminalGroup {
            version: LAYOUT_VERSION,
            root: SerializedTileTree::Axis {
                axis: SerializedAxis::Horizontal,
                flexes: vec![1., 1., 1.],
                children: vec![
                    SerializedTileTree::Tile(SerializedTile::default()),
                    SerializedTileTree::Tile(SerializedTile::default()),
                    SerializedTileTree::Tile(SerializedTile::default()),
                ],
            },
            focused_tile: Some(0),
            magnified_tile: None,
            title: None,
        };

        visual.update(|window, cx| {
            group.update(cx, |group, cx| {
                group.restore_layout(&layout, window, cx);
            });
        });
        visual.run_until_parked();

        let live = group.read_with(&mut visual, |group, cx| {
            group
                .tiles()
                .iter()
                .filter(|tile| tile.read(cx).items_len() > 0)
                .count()
        });
        assert_eq!(live, 1, "only the focused tile should have started a shell");

        // Focusing a waiting tile brings it to life.
        let waiting = group.read_with(&mut visual, |group, _| group.tiles()[2].clone());
        visual.update(|window, cx| {
            group.update(cx, |group, cx| {
                group.set_active_pane(&waiting, window, cx);
            });
        });
        visual.run_until_parked();

        let live = group.read_with(&mut visual, |group, cx| {
            group
                .tiles()
                .iter()
                .filter(|tile| tile.read(cx).items_len() > 0)
                .count()
        });
        assert_eq!(live, 2, "focusing a waiting tile should start its shell");
    }

    /// Helper: a rendered group with `count` tiles in a single row.
    async fn init_row(
        cx: &mut TestAppContext,
        count: usize,
    ) -> (Entity<TerminalGroup>, VisualTestContext) {
        let (window, group) = init_group(cx).await;
        let mut visual = VisualTestContext::from_window(window.into(), cx);
        visual.simulate_resize(gpui::size(gpui::px(3200.), gpui::px(1200.)));
        visual.run_until_parked();

        for _ in 1..count {
            visual.update(|window, cx| {
                group.update(cx, |group, cx| {
                    group.split(SplitDirection::Right, window, cx);
                });
            });
            visual.run_until_parked();
        }
        (group, visual)
    }

    #[gpui::test]
    async fn test_dropping_on_the_centre_swaps_two_tiles(cx: &mut TestAppContext) {
        cx.executor().allow_parking();
        init_test(cx);

        let (group, mut visual) = init_row(cx, 3).await;
        let (first, third) = group.read_with(&mut visual, |group, _| {
            (group.tiles()[0].clone(), group.tiles()[2].clone())
        });

        visual.update(|window, cx| {
            group.update(cx, |group, cx| {
                group.apply_drop(first.clone(), third.clone(), DropZone::Swap, window, cx);
            });
        });
        visual.run_until_parked();

        group.read_with(&mut visual, |group, _| {
            let tiles = group.tiles();
            assert_eq!(tiles.len(), 3, "a swap must not change the tile count");
            assert_eq!(tiles[0], &third, "the tiles should have exchanged places");
            assert_eq!(tiles[2], &first);
        });
    }

    #[gpui::test]
    async fn test_dropping_on_an_edge_moves_the_tile_beside_the_target(cx: &mut TestAppContext) {
        cx.executor().allow_parking();
        init_test(cx);

        let (group, mut visual) = init_row(cx, 3).await;
        let (first, second) = group.read_with(&mut visual, |group, _| {
            (group.tiles()[0].clone(), group.tiles()[1].clone())
        });

        // Move the leftmost tile below the middle one: it leaves the root row
        // and becomes a nested column.
        visual.update(|window, cx| {
            group.update(cx, |group, cx| {
                group.apply_drop(
                    first.clone(),
                    second.clone(),
                    DropZone::Edge(SplitDirection::Down),
                    window,
                    cx,
                );
            });
        });
        visual.run_until_parked();

        group.read_with(&mut visual, |group, _| {
            assert_eq!(group.tiles().len(), 3, "moving must not lose a tile");
            let Member::Axis(root) = &group.center.root else {
                panic!("expected a row at the root");
            };
            assert_eq!(
                root.members.len(),
                2,
                "the root row should have lost a member"
            );
            assert!(
                root.members
                    .iter()
                    .any(|member| matches!(member, Member::Axis(_))),
                "the target should now hold a nested column"
            );
        });
    }

    #[gpui::test]
    async fn test_dropping_on_the_outer_band_promotes_to_the_root_axis(cx: &mut TestAppContext) {
        cx.executor().allow_parking();
        init_test(cx);

        let (group, mut visual) = init_row(cx, 3).await;
        let (first, second) = group.read_with(&mut visual, |group, _| {
            (group.tiles()[0].clone(), group.tiles()[1].clone())
        });

        // Nest a tile, then promote it back out to the root row.
        visual.update(|window, cx| {
            group.update(cx, |group, cx| {
                group.apply_drop(
                    first.clone(),
                    second.clone(),
                    DropZone::Edge(SplitDirection::Down),
                    window,
                    cx,
                );
            });
        });
        visual.run_until_parked();

        visual.update(|window, cx| {
            group.update(cx, |group, cx| {
                group.apply_drop(
                    first.clone(),
                    first.clone(),
                    DropZone::Outer(SplitDirection::Right),
                    window,
                    cx,
                );
            });
        });
        visual.run_until_parked();

        group.read_with(&mut visual, |group, _| {
            assert_eq!(group.tiles().len(), 3);
            let Member::Axis(root) = &group.center.root else {
                panic!("expected a row at the root");
            };
            assert!(
                matches!(root.members.last(), Some(Member::Pane(pane)) if pane == &first),
                "the promoted tile should sit at the right edge of the root row"
            );
        });
    }

    /// A drop relocates an existing tile, so it must succeed even where a
    /// *split* would be refused. Refusing a move would leave a dense grid
    /// impossible to rearrange while preventing nothing: no tile is created.
    #[gpui::test]
    async fn test_a_cramped_drop_still_moves_the_tile(cx: &mut TestAppContext) {
        cx.executor().allow_parking();
        init_test(cx);

        let (group, mut visual) = init_row(cx, 3).await;

        // Shrink the window until no split could possibly fit.
        visual.simulate_resize(gpui::size(gpui::px(560.), gpui::px(260.)));
        visual.run_until_parked();

        let (first, second) = group.read_with(&mut visual, |group, _| {
            (group.tiles()[0].clone(), group.tiles()[1].clone())
        });

        // A split here is refused...
        group.read_with(&mut visual, |group, cx| {
            if let Some(metrics) = group.metrics_for(&second, cx) {
                let settings = *TerminalWorkspaceSettings::get_global(cx);
                assert_eq!(
                    resolve_split(
                        metrics,
                        SplitDirection::Down,
                        settings.minimum,
                        settings.split_guard
                    ),
                    SplitOutcome::Refused,
                    "this test needs a target too cramped to split"
                );
            }
        });

        // ...but the move must still happen.
        visual.update(|window, cx| {
            group.update(cx, |group, cx| {
                group.apply_drop(
                    first.clone(),
                    second.clone(),
                    DropZone::Edge(SplitDirection::Down),
                    window,
                    cx,
                );
            });
        });
        visual.run_until_parked();

        group.read_with(&mut visual, |group, _| {
            assert_eq!(group.tiles().len(), 3, "a move must not lose a tile");
            let Member::Axis(root) = &group.center.root else {
                panic!("expected a row at the root");
            };
            assert_eq!(
                root.members.len(),
                2,
                "the dragged tile should have left the root row"
            );
        });
    }

    /// The tile header's `+` is the only mouse-driven way to grow a grid, so
    /// it must go through the same guarded split as the keyboard.
    #[gpui::test]
    async fn test_the_header_plus_button_adds_a_tile(cx: &mut TestAppContext) {
        cx.executor().allow_parking();
        init_test(cx);

        let (group, mut visual) = init_row(cx, 2).await;
        let first = group.read_with(&mut visual, |group, _| group.tiles()[0].clone());

        visual.update(|window, cx| {
            group.update(cx, |group, cx| {
                group.split_tile(&first, window, cx);
            });
        });
        visual.run_until_parked();

        group.read_with(&mut visual, |group, _| {
            assert_eq!(group.tiles().len(), 3);
        });
    }

    /// A terminal dragged in from outside gets a tile of its own, rather than
    /// being filed as a sibling tab by the pane hosting the group.
    #[gpui::test]
    async fn test_a_terminal_dragged_in_gets_its_own_tile(cx: &mut TestAppContext) {
        cx.executor().allow_parking();
        init_test(cx);

        let (group, mut visual) = init_row(cx, 2).await;

        // A terminal living in a pane outside the group.
        let outside = visual.update(|window, cx| {
            group.update(cx, |group, cx| {
                let pane = group.new_tile(window, cx);
                group
                    .spawn_terminal_into(pane.clone(), None, None, window, cx)
                    .detach();
                pane
            })
        });
        visual.run_until_parked();

        let item = outside
            .read_with(&mut visual, |pane, _| pane.active_item())
            .expect("the outside pane should hold a terminal");

        visual.update(|window, cx| {
            group.update(cx, |group, cx| {
                group.adopt_terminal(item, window, cx);
            });
        });
        visual.run_until_parked();

        group.read_with(&mut visual, |group, cx| {
            assert_eq!(group.tiles().len(), 3, "the terminal should become a tile");
            for tile in group.tiles() {
                assert_eq!(
                    tile.read(cx).items_len(),
                    1,
                    "every tile still holds exactly one terminal"
                );
            }
        });
    }

    #[gpui::test]
    async fn test_dropping_a_tile_on_itself_changes_nothing(cx: &mut TestAppContext) {
        cx.executor().allow_parking();
        init_test(cx);

        let (group, mut visual) = init_row(cx, 3).await;
        let before = group.read_with(&mut visual, |group, _| {
            group.tiles().into_iter().cloned().collect::<Vec<_>>()
        });
        let first = before[0].clone();

        for zone in [DropZone::Swap, DropZone::Edge(SplitDirection::Right)] {
            visual.update(|window, cx| {
                group.update(cx, |group, cx| {
                    group.apply_drop(first.clone(), first.clone(), zone, window, cx);
                });
            });
            visual.run_until_parked();
        }

        group.read_with(&mut visual, |group, _| {
            let after = group.tiles().into_iter().cloned().collect::<Vec<_>>();
            assert_eq!(after, before, "a self-drop must leave the tree untouched");
        });
    }

    #[gpui::test]
    async fn test_swapping_exchanges_neighbouring_tiles(cx: &mut TestAppContext) {
        cx.executor().allow_parking();
        init_test(cx);

        let (group, mut visual) = init_row(cx, 3).await;
        let before = group.read_with(&mut visual, |group, _| {
            group.tiles().into_iter().cloned().collect::<Vec<_>>()
        });

        // Focus the middle tile, then swap it leftward.
        visual.update(|window, cx| {
            group.update(cx, |group, cx| {
                let middle = group.tiles()[1].clone();
                group.set_active_pane(&middle, window, cx);
                group.swap_in_direction(SplitDirection::Left, window, cx);
            });
        });
        visual.run_until_parked();

        group.read_with(&mut visual, |group, _| {
            let after = group.tiles();
            assert_eq!(after[0], &before[1]);
            assert_eq!(after[1], &before[0]);
            assert_eq!(after[2], &before[2], "the far tile should not move");
        });
    }

    #[gpui::test]
    async fn test_equalize_restores_equal_shares(cx: &mut TestAppContext) {
        cx.executor().allow_parking();
        init_test(cx);

        let (group, mut visual) = init_row(cx, 3).await;

        group.read_with(&mut visual, |group, _| {
            let Member::Axis(axis) = &group.center.root else {
                panic!("expected a row of tiles");
            };
            *axis.flexes.lock() = vec![2.0, 0.6, 0.4];
        });

        visual.update(|_window, cx| {
            group.update(cx, |group, cx| group.equalize(cx));
        });
        visual.run_until_parked();

        group.read_with(&mut visual, |group, _| {
            let Member::Axis(axis) = &group.center.root else {
                panic!("expected a row of tiles");
            };
            for flex in axis.flexes.lock().iter() {
                assert!(
                    (flex - 1.).abs() < 0.001,
                    "every tile should hold an equal share, got {flex}"
                );
            }
        });
    }

    #[gpui::test]
    async fn test_tab_title_reports_tile_count(cx: &mut TestAppContext) {
        cx.executor().allow_parking();
        init_test(cx);

        let (window, group) = init_group(cx).await;

        group.read_with(cx, |group, cx| {
            assert_eq!(group.tab_content_text(0, cx).as_ref(), "Terminal");
        });

        window
            .update(cx, |_, window, cx| {
                group.update(cx, |group, cx| {
                    group.split(SplitDirection::Right, window, cx);
                });
            })
            .expect("failed to split");
        cx.run_until_parked();

        group.read_with(cx, |group, cx| {
            assert_eq!(group.tab_content_text(0, cx).as_ref(), "Terminal (2)");
        });
    }
}
