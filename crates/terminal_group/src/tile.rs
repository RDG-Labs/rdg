use std::any::Any;
use std::sync::Arc;

use gpui::{Action as _, AnyElement, Entity, WeakEntity};
use terminal_view::TerminalView;
use ui::prelude::*;
use ui::{ContextMenu, IconButton, IconButtonShape, IconName, IconSize, Label, LabelSize, PopoverMenu, Tooltip};
use workspace::{Pane, Workspace};

use crate::{DraggedTile, TerminalGroup, installed_agents};

/// Height of a tile header. Charged against the usable area by the split guard,
/// so the two must agree.
pub(crate) const HEADER_HEIGHT: f32 = 24.;

/// Builds a pane configured as a tile: no tab strip, no navigation, no drop
/// targets, and a slim header in place of the tab bar.
///
/// The pane is still a full `Pane`, so splitting, resizing, axis collapse, and
/// item lifecycle all come from code Zed already exercises. What a tile adds is
/// the presentation and the one-terminal invariant.
pub(crate) fn new_tile_pane(
    workspace: WeakEntity<Workspace>,
    project: Entity<project::Project>,
    group: WeakEntity<TerminalGroup>,
    window: &mut Window,
    cx: &mut Context<TerminalGroup>,
) -> Entity<Pane> {
    cx.new(|cx| {
        let mut pane = Pane::new(
            workspace,
            project,
            Default::default(),
            None,
            workspace::NewTerminal::default().boxed_clone(),
            false,
            window,
            cx,
        );

        pane.set_can_navigate(false, cx);
        pane.display_nav_history_buttons(None);
        pane.set_can_toggle_zoom(false, cx);
        pane.set_zoom_out_on_close(false);

        // The tab bar slot renders the tile header instead. `should_display_tab_bar`
        // must stay true or the slot is never rendered at all.
        pane.set_should_display_tab_bar(|_, _| true);
        pane.set_render_tab_bar(cx, {
            let group = group.clone();
            move |pane, window, cx| render_tile_header(&group, pane, window, cx)
        });

        // Invariant TG-1: nothing may be dropped into a tile. A tile holds
        // exactly one terminal; anything else becomes its own tile.
        pane.set_can_split(Some(Arc::new(
            |_pane: &mut Pane, _dragged: &dyn Any, _window: &mut Window, _cx: &mut Context<Pane>| {
                false
            },
        )));

        pane
    })
}

/// The terminal a tile holds, if it has one yet.
pub(crate) fn tile_terminal(pane: &Pane, _cx: &App) -> Option<Entity<TerminalView>> {
    pane.active_item()
        .and_then(|item| item.downcast::<TerminalView>())
}

/// Title for a tile, resolved in priority order: the foreground process, then
/// the terminal's own title, then a fallback.
///
/// The cwd basename is prefixed when it differs from the terminal's title, so a
/// wall of shells is distinguishable at a glance.
fn tile_title(terminal_view: &Entity<TerminalView>, cx: &App) -> SharedString {
    let terminal = terminal_view.read(cx).terminal().read(cx);

    let process = terminal.foreground_process_command_name();
    let title = process.unwrap_or_else(|| terminal.title(true));

    let directory = terminal
        .working_directory()
        .and_then(|path| path.file_name().map(|name| name.to_string_lossy().to_string()));

    match directory {
        Some(directory) if !title.contains(&directory) => {
            SharedString::from(format!("{directory} ▸ {title}"))
        }
        _ => SharedString::from(title),
    }
}

/// Running tiles get an accent dot; idle tiles a muted one. A tile at a bare
/// prompt is doing nothing, and should not draw the eye.
fn tile_is_running(terminal_view: &Entity<TerminalView>, cx: &App) -> bool {
    terminal_view
        .read(cx)
        .terminal()
        .read(cx)
        .foreground_process_command_name()
        .is_some()
}

fn render_tile_header(
    group: &WeakEntity<TerminalGroup>,
    pane: &mut Pane,
    _window: &mut Window,
    cx: &mut Context<Pane>,
) -> AnyElement {
    let terminal_view = tile_terminal(pane, cx);
    let focused = group
        .read_with(cx, |group, _| group.is_active_pane(&cx.entity()))
        .unwrap_or(false);
    let magnified = group
        .read_with(cx, |group, _| group.is_magnified(&cx.entity()))
        .unwrap_or(false);

    let title = terminal_view
        .as_ref()
        .map(|terminal_view| tile_title(terminal_view, cx))
        .unwrap_or_else(|| SharedString::from("Terminal"));
    let running = terminal_view
        .as_ref()
        .is_some_and(|terminal_view| tile_is_running(terminal_view, cx));

    let pane_entity = cx.entity();

    h_flex()
        .id("tile-header")
        .on_drag(
            DraggedTile {
                pane: pane_entity.clone(),
                group: group.clone(),
                title: title.clone(),
            },
            |dragged, _, _, cx| cx.new(|_| dragged.clone()),
        )
        .h(px(HEADER_HEIGHT))
        .w_full()
        .flex_none()
        .px_2()
        .gap_1p5()
        .bg(if focused {
            ui::glass_elevated_color(cx.theme().colors().tab_bar_background, cx)
        } else {
            ui::glass_surface_color(cx.theme().colors().tab_bar_background, cx)
        })
        .border_b_1()
        .border_color(ui::glass_border_color(cx.theme().colors().border, cx))
        .child(
            div()
                .size(px(6.))
                .rounded_full()
                .flex_none()
                .bg(if running {
                    cx.theme().status().info
                } else {
                    cx.theme().colors().text_disabled
                }),
        )
        .child(
            Label::new(title)
                .size(LabelSize::Small)
                .color(if focused { Color::Default } else { Color::Muted })
                .truncate(),
        )
        .child(div().flex_1())
        .child({
            let group = group.clone();
            let pane_entity = pane_entity.clone();
            PopoverMenu::new("split-tile")
                .trigger_with_tooltip(
                    IconButton::new("split-tile", IconName::Plus)
                        .shape(IconButtonShape::Square)
                        .icon_size(IconSize::XSmall),
                    Tooltip::text("New terminal or coding agent"),
                )
                .menu(move |window, cx| {
                    let group = group.upgrade()?;
                    let agents = installed_agents();
                    let pane_entity = pane_entity.clone();
                    Some(ContextMenu::build(window, cx, move |menu, window, _| {
                        let pane_for_terminal = pane_entity.clone();
                        let group_for_terminal = group.clone();
                        let mut menu = menu.entry(
                            "New Terminal",
                            None,
                            window.handler_for(&group_for_terminal, move |group, window, cx| {
                                group.split_tile(&pane_for_terminal, window, cx);
                            }),
                        );
                        let pane_for_custom_command = pane_entity.clone();
                        let group_for_custom_command = group.clone();
                        menu = menu.separator().entry(
                            "Custom Command…",
                            Some(crate::CustomCommand.boxed_clone()),
                            window.handler_for(&group_for_custom_command, move |group, window, cx| {
                                group.prompt_custom_command(
                                    pane_for_custom_command.clone(),
                                    window,
                                    cx,
                                );
                            }),
                        );
                        if !agents.is_empty() {
                            menu = menu.separator().label("Installed Agents");
                            for agent in &agents {
                                let pane = pane_entity.clone();
                                let group = group.clone();
                                let command = agent.command.to_owned();
                                menu = menu.entry(
                                    agent.name,
                                    None,
                                    window.handler_for(&group, move |group, window, cx| {
                                        group.spawn_agent_beside(&pane, command.clone(), window, cx);
                                    }),
                                );
                            }
                        }

                        menu
                    }))
                })
        })
        .child(
            IconButton::new("magnify-tile", if magnified {
                IconName::Minimize
            } else {
                IconName::Maximize
            })
            .shape(IconButtonShape::Square)
            .icon_size(IconSize::XSmall)
            .tooltip(Tooltip::text(if magnified {
                "Restore tile"
            } else {
                "Magnify tile"
            }))
            .on_click({
                let group = group.clone();
                let pane_entity = pane_entity.clone();
                move |_, window, cx| {
                    group
                        .update(cx, |group, cx| {
                            group.toggle_magnify(&pane_entity, window, cx);
                        })
                        .ok();
                }
            }),
        )
        .child(
            IconButton::new("close-tile", IconName::Close)
                .shape(IconButtonShape::Square)
                .icon_size(IconSize::XSmall)
                .tooltip(Tooltip::text("Close tile"))
                .on_click({
                    let group = group.clone();
                    move |_, window, cx| {
                        group
                            .update(cx, |group, cx| {
                                group.close_tile(&pane_entity, window, cx);
                            })
                            .ok();
                    }
                }),
        )
        .into_any_element()
}
