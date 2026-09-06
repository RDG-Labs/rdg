//! Tile-count scaling benchmark for terminal-group layout (production
//! `bench-support` seam, no `test-support`).
//!
//! A `PaneGroup`'s leaves are `Entity<Pane>`s, which require a `Workspace` to
//! construct. Setup mints real `Pane` handles by splitting a real `Workspace`
//! (discarded afterward), then the measured path drives an isolated `PaneGroup`
//! directly — split needs only `&mut App`, so the hot loop runs without a
//! `Window`. This is the structural-growth cost that bounds how many visible
//! terminals a group can hold, and therefore where worker overflow begins.
use std::sync::Arc;

use fs::RealFs;
use gpui::{
    App, AppContext as _, BenchAppContext, BenchWindowContext, Entity, InteractiveElement,
    IntoElement, ParentElement, Render, Styled, VisualContext, div,
};
use language::LanguageRegistry;
use project::Project;
use terminal::{
    Terminal, TerminalBuilder, terminal_settings::{AlternateScroll, CursorShape},
};
use terminal_view::TerminalView;
use workspace::{AppState, MultiWorkspace, Pane, PaneGroup, SplitDirection, Workspace};

fn init_globals(cx: &mut App) -> Arc<AppState> {
    // Production settings bootstrap loads embedded `settings/default.json`;
    // `settings::init` also installs the `SettingsStore` global.
    settings::init(cx);
    theme_settings::init(theme::LoadThemes::JustBase, cx);

    // An in-memory database (production migrations, `bench-support` token);
    // avoids touching the user's real db and keeps the graph `test-support`-free.
    if !cx.has_global::<db::AppDatabase>() {
        cx.set_global(db::AppDatabase::bench_new());
    }

    // `BenchAppContext` already installed a `BlockedHttpClient`, so no network.
    let client = client::Client::production(cx);
    let fs = Arc::new(RealFs::new(None, cx.background_executor().clone()));
    let languages = Arc::new(LanguageRegistry::new(cx.background_executor().clone()));
    let app_state = AppState::bench(client, fs, languages, cx);
    AppState::set_global(app_state.clone(), cx);

    client::init(&app_state.client, cx);
    Project::init(&app_state.client, cx);
    editor::init(cx);
    terminal_view::init(cx);
    terminal_group::init(cx);
    workspace::init(app_state.clone(), cx);

    app_state
}

/// Builds a real project + workspace and mints `panes` real `Pane` handles by
/// splitting. The workspace is discarded; the panes feed the measured group.
fn mint_panes(panes: usize, cx: &mut BenchAppContext) -> Vec<Entity<Pane>> {
    let app_state = cx.update(|cx| init_globals(cx));
    let project = cx.update(|cx| {
        Project::local(
            app_state.client.clone(),
            app_state.node_runtime.clone(),
            app_state.user_store.clone(),
            app_state.languages.clone(),
            app_state.fs.clone(),
            None,
            project::LocalProjectFlags::default(),
            cx,
        )
    });

    let mut window = cx.add_empty_window();
    let multi = window
        .new_window_entity(|window, cx| {
            let workspace = cx.new(|cx| Workspace::new(None, project, app_state, window, cx));
            MultiWorkspace::new(workspace, window, cx)
        })
        .expect("failed to create multi-workspace");
    let workspace = multi.read_with(cx, |m, _| m.workspace().clone());

    // Mint panes by splitting the first pane repeatedly. Each split returns a
    // real `Entity<Pane>` we can hand to the isolated `PaneGroup`. Splitting
    // needs `&mut Window`, so route through `update_window_entity`; the extra
    // splits on the (discarded) workspace group are irrelevant.
    for _ in 1..panes {
        window
            .update_window_entity(&workspace, |ws, window, cx| {
                let active = ws.active_pane().clone();
                ws.split_pane(active, SplitDirection::Right, window, cx);
            })
            .expect("failed to split workspace pane");
    }
    window.run_until_idle();

    workspace.read_with(cx, |ws, _| ws.panes().to_vec())
}

#[gpui::bench(inputs = pane_count_inputs(), input_name = "panes", group = "Grid split")]
fn grid_split_scaling(panes: &usize, cx: &mut BenchAppContext) {
    let panes = mint_panes(*panes, cx);
    let first = panes[0].clone();

    cx.bench_iter(move |cx| {
        cx.update(|cx| {
            // Grow a fresh single-pane group to `panes` every iteration; setup
            // is the same each time so only the tree growth is measured.
            let mut group = PaneGroup::new(first.clone());
            for pair in panes.windows(2) {
                group.split(&pair[0], &pair[1], SplitDirection::Right, cx);
            }
        });
    });
}

fn pane_count_inputs() -> Vec<usize> {
    let mut counts = vec![12, 24];
    if std::env::var("ZED_BENCH_HUGE").is_ok() {
        counts.push(40);
    }
    counts
}

/// The window root for the terminal-render benchmark: a grid of N real
/// `TerminalView`s laid out vertically, each painting real terminal content.
/// Repainting this is the frame cost a tiled grid pays per frame.
struct TileGrid {
    tiles: Vec<Entity<TerminalView>>,
    /// Terminal handles for streaming writes; the first `streaming_count`
    /// tiles receive continuous output each frame.
    terminals: Vec<Entity<Terminal>>,
    streaming_count: usize,
}

impl Render for TileGrid {
    fn render(&mut self, _window: &mut gpui::Window, _cx: &mut gpui::Context<Self>) -> impl IntoElement {
        div().flex().flex_col().size_full().id("tile-grid")
            .children(self.tiles.iter().cloned().collect::<Vec<_>>())
    }
}

/// Builds a grid of `tile_count` real display-only terminals as the window
/// root. With `streaming_count > 0`, that many leading tiles stream continuous
/// output when the grid's update callback runs.
fn build_grid(
    tile_count: usize,
    streaming_count: usize,
    app_state: &Arc<AppState>,
    window: &mut BenchWindowContext,
) -> Entity<TileGrid> {
    let grid = window
        .update(|window, cx| {
            let project = Project::local(
                app_state.client.clone(),
                app_state.node_runtime.clone(),
                app_state.user_store.clone(),
                app_state.languages.clone(),
                app_state.fs.clone(),
                None,
                project::LocalProjectFlags::default(),
                cx,
            );
            let workspace = cx.new(|cx| Workspace::new(None, project, app_state.clone(), window, cx));
            let project_weak = workspace.read(cx).project().downgrade();
            let workspace_weak = workspace.downgrade();

            let mut tiles = Vec::with_capacity(tile_count);
            let mut terminals = Vec::with_capacity(tile_count);
            for index in 0..tile_count {
                let terminal = cx.new(|cx| {
                    TerminalBuilder::new_display_only(
                        CursorShape::Block,
                        AlternateScroll::On,
                        None,
                        0,
                        cx.background_executor(),
                        util::paths::PathStyle::local(),
                    )
                    .subscribe(cx)
                });
                terminal.update(cx, |terminal, cx| {
                    let line = format!("[api:{index} INFO] request handled in 12ms\n");
                    terminal.write_output(line.as_bytes(), cx);
                });
                let tile = cx.new(|cx| {
                    TerminalView::new(
                        terminal.clone(),
                        workspace_weak.clone(),
                        None,
                        project_weak.clone(),
                        window,
                        cx,
                    )
                });
                tiles.push(tile);
                terminals.push(terminal);
            }
            window.replace_root(cx, |_, _cx| TileGrid {
                tiles,
                terminals,
                streaming_count,
            })
        });
    grid
}

/// Frame cost of a grid of `tile_count` visible terminals. Each iteration the
/// root forces a full repaint of every tile; this is the steady-state idle-grid
/// frame cost PRD §8.1 drives the visible-tile cap with.
#[gpui::bench(inputs = repaint_inputs(), input_name = "tiles", group = "Grid frame")]
fn grid_frame_scaling(tile_count: &usize, cx: &mut BenchAppContext) {
    let app_state = cx.update(|cx| init_globals(cx));
    let mut window = cx.add_empty_window();

    // Build a static grid: no terminal writes, only repaint.
    let grid = build_grid(*tile_count, 0, &app_state, &mut window);

    cx.bench_renderer(grid, |_this, _window, cx| {
        // Force a full repaint of every tile each iteration (steady-state idle
        // frame cost, PRD §8.1).
        cx.notify();
    });
}

/// Frame cost of a grid where `streaming()` tiles receive continuous output
/// (≈1000 lines/s each) — the PRD §8.1 "streaming" workload. The remaining
/// tiles stay idle, so this stresses repaint under mixed load.
#[gpui::bench(inputs = repaint_inputs(), input_name = "tiles", group = "Grid frame")]
fn grid_frame_streaming_scaling(tile_count: &usize, cx: &mut BenchAppContext) {
    let app_state = cx.update(|cx| init_globals(cx));
    let mut window = cx.add_empty_window();

    // ~1/3 of tiles stream (rounds to 4 of 12, 8 of 24, 13 of 40).
    let streaming_count = (*tile_count / 3).max(1);
    let grid = build_grid(*tile_count, streaming_count, &app_state, &mut window);

    cx.bench_renderer(grid, |this, _window, cx| {
        // Feed a line to each streaming terminal, then repaint — models a
        // busy agent that doesn't hold the frame budget hostage.
        for terminal in &this.terminals[..this.streaming_count] {
            terminal.update(cx, |terminal, cx| {
                terminal.write_output(b"[api] stream line 0000 \\x1b[2K INFO done\n", cx);
            });
        }
        cx.notify();
    });
}

fn repaint_inputs() -> Vec<usize> {
    let mut counts = vec![12, 24];
    if std::env::var("ZED_BENCH_HUGE").is_ok() {
        counts.push(40);
    }
    counts
}

gpui::bench_group!(benches, grid_split_scaling, grid_frame_scaling, grid_frame_streaming_scaling);
gpui::bench_main!(benches);