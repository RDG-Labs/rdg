use anyhow::Result;
use editor::Editor;
use gpui::{
    Action as _, App, AppContext as _, Context, Entity, EventEmitter, FocusHandle, Focusable,
    Render, SharedString, Subscription, WeakEntity, Window,
};
use language::Buffer;
use project::Project;
use std::any::TypeId;
use workspace::item::{Item, ItemBufferKind, ItemEvent, SaveOptions};
use workspace::{ActivePaneDecorator, NewFile, Pane, PaneGroup, SplitDirection, Workspace};

use crate::markdown_preview_view::{MarkdownPreviewEvent, MarkdownPreviewView};

/// A Markdown editor and preview rendered as two panes inside one workspace tab.
pub struct MarkdownPreviewGroup {
    pane_group: PaneGroup,
    editor: Entity<Editor>,
    preview: Entity<MarkdownPreviewView>,
    active_pane: Entity<Pane>,
    workspace: WeakEntity<Workspace>,
    focus_handle: FocusHandle,
    _subscriptions: Vec<Subscription>,
}

impl MarkdownPreviewGroup {
    pub fn new(
        workspace: &mut Workspace,
        editor: Entity<Editor>,
        preview: Entity<MarkdownPreviewView>,
        window: &mut Window,
        cx: &mut Context<Workspace>,
    ) -> Entity<Self> {
        let workspace_handle = workspace.weak_handle();
        let project = workspace.project().clone();
        let group = cx.new(|cx| {
            let editor_pane = Self::new_pane(workspace_handle.clone(), project.clone(), window, cx);
            let preview_pane = Self::new_pane(workspace_handle.clone(), project, window, cx);
            let mut pane_group = PaneGroup::new(editor_pane.clone());
            pane_group.split(&editor_pane, &preview_pane, SplitDirection::Right, cx);
            let focus_handle = cx.focus_handle();
            Self {
                pane_group,
                editor,
                preview,
                active_pane: editor_pane,
                workspace: workspace_handle,
                focus_handle,
                _subscriptions: Vec::new(),
            }
        });

        group.update(cx, |group, cx| {
            let editor_pane = group.pane_group.first_pane();
            let preview_pane = group.pane_group.last_pane();
            editor_pane.update(cx, |pane, cx| {
                pane.add_item(Box::new(group.editor.clone()), true, true, None, window, cx);
            });
            preview_pane.update(cx, |pane, cx| {
                pane.add_item(
                    Box::new(group.preview.clone()),
                    false,
                    false,
                    None,
                    window,
                    cx,
                );
            });
            group
                ._subscriptions
                .push(cx.subscribe(&editor_pane, Self::pane_event));
            group
                ._subscriptions
                .push(cx.subscribe(&preview_pane, Self::pane_event));
        });
        group
    }

    fn new_pane(
        workspace: WeakEntity<Workspace>,
        project: Entity<Project>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<Pane> {
        cx.new(|cx| {
            let mut pane = Pane::new(
                workspace,
                project,
                Default::default(),
                None,
                NewFile.boxed_clone(),
                true,
                window,
                cx,
            );
            pane.set_should_display_tab_bar(|_, _| false);
            pane.set_can_navigate(false, cx);
            pane.set_can_toggle_zoom(false, cx);
            pane.set_zoom_out_on_close(false);
            pane
        })
    }

    fn pane_event(
        group: &mut Self,
        pane: Entity<Pane>,
        event: &workspace::pane::Event,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if matches!(event, workspace::pane::Event::Focus) {
            group.active_pane = pane;
            window.focus(&group.active_pane.focus_handle(cx), cx);
            cx.notify();
        }
    }

    pub fn editor(&self) -> Entity<Editor> {
        self.editor.clone()
    }

    pub fn is_previewing(&self, buffer: &Entity<Buffer>, cx: &App) -> bool {
        self.editor
            .read(cx)
            .buffer()
            .read(cx)
            .as_singleton()
            .as_ref()
            == Some(buffer)
    }
}

impl Focusable for MarkdownPreviewGroup {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EventEmitter<MarkdownPreviewEvent> for MarkdownPreviewGroup {}

impl Render for MarkdownPreviewGroup {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl gpui::IntoElement {
        let decorator = ActivePaneDecorator::new(&self.active_pane, &self.workspace);
        self.pane_group.render(None, None, &decorator, window, cx)
    }
}

impl Item for MarkdownPreviewGroup {
    type Event = MarkdownPreviewEvent;

    fn act_as_type<'a>(
        &'a self,
        type_id: TypeId,
        self_handle: &'a Entity<Self>,
        _cx: &'a App,
    ) -> Option<gpui::AnyEntity> {
        if type_id == TypeId::of::<Self>() {
            Some(self_handle.clone().into())
        } else if type_id == TypeId::of::<Editor>() {
            Some(self.editor.clone().into())
        } else {
            None
        }
    }

    fn tab_content_text(&self, _detail: usize, cx: &App) -> SharedString {
        self.editor.read(cx).buffer().read(cx).title(cx).into()
    }

    fn tab_icon(&self, _window: &Window, _cx: &App) -> Option<ui::Icon> {
        Some(ui::Icon::new(ui::IconName::FileDoc))
    }

    fn telemetry_event_text(&self) -> Option<&'static str> {
        Some("Markdown Editor and Preview Opened")
    }

    fn to_item_events(event: &Self::Event, f: &mut dyn FnMut(ItemEvent)) {
        MarkdownPreviewView::to_item_events(event, f);
    }

    fn can_save(&self, cx: &App) -> bool {
        self.editor.read(cx).can_save(cx)
    }

    fn can_save_as(&self, cx: &App) -> bool {
        self.editor.read(cx).can_save_as(cx)
    }

    fn save(
        &mut self,
        options: SaveOptions,
        project: Entity<Project>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::Task<Result<()>> {
        self.editor
            .update(cx, |editor, cx| editor.save(options, project, window, cx))
    }

    fn save_as(
        &mut self,
        project: Entity<Project>,
        path: project::ProjectPath,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::Task<Result<()>> {
        self.editor
            .update(cx, |editor, cx| editor.save_as(project, path, window, cx))
    }

    fn buffer_kind(&self, _cx: &App) -> ItemBufferKind {
        ItemBufferKind::Singleton
    }
}
