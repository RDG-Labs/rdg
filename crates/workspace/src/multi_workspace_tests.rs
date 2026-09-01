use std::path::PathBuf;

use super::*;
use client::proto;
use fs::{FakeFs, Fs};
use gpui::{TestAppContext, VisualTestContext};
use project::DisableAiSettings;
use serde_json::json;
use settings::{Settings, SettingsStore};
use util::path;

fn init_test(cx: &mut TestAppContext) {
    cx.update(|cx| {
        let settings_store = SettingsStore::test(cx);
        cx.set_global(settings_store);
        theme_settings::init(theme::LoadThemes::JustBase, cx);
        DisableAiSettings::register(cx);
    });
}

fn setup_multi_workspace<'a>(
    projects: &[Entity<Project>],
    cx: &'a mut TestAppContext,
) -> (Entity<MultiWorkspace>, &'a mut VisualTestContext) {
    let mut iterator = projects.iter();
    let project = iterator
        .next()
        .expect("At least one project should be provided")
        .clone();

    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project, window, cx));

    for project in iterator {
        multi_workspace.update_in(cx, |multi_workspace, window, cx| {
            multi_workspace.test_add_workspace(project.clone(), window, cx);
        })
    }

    // Opening the sidebar retains the workspaces and establishes their project groups.
    multi_workspace.update(cx, |multi_workspace, cx| multi_workspace.open_sidebar(cx));
    cx.run_until_parked();

    (multi_workspace, cx)
}

#[gpui::test]
async fn test_project_group_keys_initial(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());
    fs.insert_tree("/root_a", json!({ "file.txt": "" })).await;
    let project = Project::test(fs, ["/root_a".as_ref()], cx).await;

    let expected_key = project.read_with(cx, |project, cx| project.project_group_key(cx));

    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project, window, cx));

    multi_workspace.update(cx, |mw, cx| {
        mw.open_sidebar(cx);
    });

    multi_workspace.read_with(cx, |mw, _cx| {
        let keys: Vec<ProjectGroupKey> = mw.project_group_keys();
        assert_eq!(keys.len(), 1, "should have exactly one key on creation");
        assert_eq!(keys[0], expected_key);
    });
}

#[gpui::test]
async fn test_open_new_window_does_not_open_sidebar_on_existing_window(cx: &mut TestAppContext) {
    init_test(cx);

    let app_state = cx.update(AppState::test);
    let fs = app_state.fs.as_fake();
    fs.insert_tree(path!("/project_a"), json!({ "file.txt": "" }))
        .await;
    fs.insert_tree(path!("/project_b"), json!({ "file.txt": "" }))
        .await;

    let project = Project::test(app_state.fs.clone(), [path!("/project_a").as_ref()], cx).await;

    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project, window, cx));

    window
        .read_with(cx, |mw, _cx| {
            assert!(!mw.sidebar_open(), "sidebar should start closed",);
        })
        .unwrap();

    cx.update(|cx| {
        open_paths(
            &[PathBuf::from(path!("/project_b"))],
            app_state,
            OpenOptions {
                open_mode: OpenMode::NewWindow,
                ..OpenOptions::default()
            },
            cx,
        )
    })
    .await
    .unwrap();

    window
        .read_with(cx, |mw, _cx| {
            assert!(
                !mw.sidebar_open(),
                "opening a project in a new window must not open the sidebar on the original window",
            );
        })
        .unwrap();
}

#[gpui::test]
async fn test_open_directory_in_empty_workspace_does_not_open_sidebar(cx: &mut TestAppContext) {
    init_test(cx);

    let app_state = cx.update(AppState::test);
    let fs = app_state.fs.as_fake();
    fs.insert_tree(path!("/project"), json!({ "file.txt": "" }))
        .await;

    let project = Project::test(app_state.fs.clone(), [], cx).await;
    let window = cx.add_window(|window, cx| {
        let mw = MultiWorkspace::test_new(project, window, cx);
        // Simulate a blank project that has an untitled editor tab,
        // so that workspace_windows_for_location finds this window.
        mw.workspace().update(cx, |workspace, cx| {
            workspace.active_pane().update(cx, |pane, cx| {
                let item = cx.new(|cx| item::test::TestItem::new(cx));
                pane.add_item(Box::new(item), false, false, None, window, cx);
            });
        });
        mw
    });

    window
        .read_with(cx, |mw, _cx| {
            assert!(!mw.sidebar_open(), "sidebar should start closed");
        })
        .unwrap();

    // Simulate what open_workspace_for_paths does for an empty workspace:
    // it downgrades OpenMode::NewWindow to Activate and sets requesting_window.
    cx.update(|cx| {
        open_paths(
            &[PathBuf::from(path!("/project"))],
            app_state,
            OpenOptions {
                requesting_window: Some(window),
                open_mode: OpenMode::Activate,
                ..OpenOptions::default()
            },
            cx,
        )
    })
    .await
    .unwrap();

    window
        .read_with(cx, |mw, _cx| {
            assert!(
                !mw.sidebar_open(),
                "opening a directory in a blank project via the file picker must not open the sidebar",
            );
        })
        .unwrap();
}

#[gpui::test]
async fn test_project_group_keys_duplicate_not_added(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());
    fs.insert_tree("/root_a", json!({ "file.txt": "" })).await;
    let project_a = Project::test(fs.clone(), ["/root_a".as_ref()], cx).await;
    // A second project entity pointing at the same path produces the same key.
    let project_a2 = Project::test(fs.clone(), ["/root_a".as_ref()], cx).await;

    let key_a = project_a.read_with(cx, |p, cx| p.project_group_key(cx));
    let key_a2 = project_a2.read_with(cx, |p, cx| p.project_group_key(cx));
    assert_eq!(key_a, key_a2, "same root path should produce the same key");

    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project_a, window, cx));

    multi_workspace.update(cx, |mw, cx| {
        mw.open_sidebar(cx);
    });

    multi_workspace.update_in(cx, |mw, window, cx| {
        mw.test_add_workspace(project_a2, window, cx);
    });

    multi_workspace.read_with(cx, |mw, _cx| {
        let keys: Vec<ProjectGroupKey> = mw.project_group_keys();
        assert_eq!(
            keys.len(),
            1,
            "duplicate key should not be added when a workspace with the same root is inserted"
        );
    });
}

#[gpui::test]
async fn test_adding_worktree_updates_project_group_key(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());
    fs.insert_tree("/root_a", json!({ "file.txt": "" })).await;
    fs.insert_tree("/root_b", json!({ "other.txt": "" })).await;
    let project = Project::test(fs.clone(), ["/root_a".as_ref()], cx).await;

    let initial_key = project.read_with(cx, |p, cx| p.project_group_key(cx));

    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));

    // Open sidebar to retain the workspace and create the initial group.
    multi_workspace.update(cx, |mw, cx| {
        mw.open_sidebar(cx);
    });
    cx.run_until_parked();

    multi_workspace.read_with(cx, |mw, _cx| {
        let keys = mw.project_group_keys();
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0], initial_key);
    });

    // Add a second worktree to the project. This triggers WorktreeAdded →
    // handle_workspace_key_change, which should update the group key.
    project
        .update(cx, |project, cx| {
            project.find_or_create_worktree("/root_b", true, cx)
        })
        .await
        .expect("adding worktree should succeed");
    cx.run_until_parked();

    let updated_key = project.read_with(cx, |p, cx| p.project_group_key(cx));
    assert_ne!(
        initial_key, updated_key,
        "adding a worktree should change the project group key"
    );

    multi_workspace.read_with(cx, |mw, _cx| {
        let keys = mw.project_group_keys();
        assert!(
            keys.contains(&updated_key),
            "should contain the updated key; got {keys:?}"
        );
    });
}

#[gpui::test]
async fn test_find_or_create_local_workspace_reuses_active_workspace_when_sidebar_closed(
    cx: &mut TestAppContext,
) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());
    fs.insert_tree("/root_a", json!({ "file.txt": "" })).await;
    let project = Project::test(fs, ["/root_a".as_ref()], cx).await;

    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project, window, cx));

    let active_workspace = multi_workspace.read_with(cx, |mw, cx| {
        assert!(
            mw.project_groups(cx).is_empty(),
            "sidebar-closed setup should start with no retained project groups"
        );
        mw.workspace().clone()
    });
    let active_workspace_id = active_workspace.entity_id();

    let workspace = multi_workspace
        .update_in(cx, |mw, window, cx| {
            mw.find_or_create_local_workspace(
                PathList::new(&[PathBuf::from("/root_a")]),
                None,
                None,
                OpenMode::Activate,
                None,
                window,
                cx,
            )
        })
        .await
        .expect("reopening the same local workspace should succeed");

    assert_eq!(
        workspace.entity_id(),
        active_workspace_id,
        "should reuse the current active workspace when the sidebar is closed"
    );

    multi_workspace.read_with(cx, |mw, _cx| {
        assert_eq!(
            mw.workspace().entity_id(),
            active_workspace_id,
            "active workspace should remain unchanged after reopening the same path"
        );
        assert_eq!(
            mw.workspaces().count(),
            1,
            "reusing the active workspace should not create a second open workspace"
        );
    });
}

#[gpui::test]
async fn test_find_or_create_workspace_uses_project_group_key_when_paths_are_missing(
    cx: &mut TestAppContext,
) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        "/project",
        json!({
            ".git": {},
            "src": {},
        }),
    )
    .await;
    cx.update(|cx| <dyn Fs>::set_global(fs.clone(), cx));
    let project = Project::test(fs.clone(), ["/project".as_ref()], cx).await;
    project
        .update(cx, |project, cx| project.git_scans_complete(cx))
        .await;

    let project_group_key = project.read_with(cx, |project, cx| project.project_group_key(cx));

    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project, window, cx));

    let main_workspace = multi_workspace.read_with(cx, |mw, _cx| mw.workspace().clone());
    let main_workspace_id = main_workspace.entity_id();

    let workspace = multi_workspace
        .update_in(cx, |mw, window, cx| {
            mw.find_or_create_workspace(
                PathList::new(&[PathBuf::from("/wt-feature-a")]),
                None,
                Some(project_group_key.clone()),
                |_options, _window, _cx| Task::ready(Ok(None)),
                None,
                OpenMode::Activate,
                None,
                window,
                cx,
            )
        })
        .await
        .expect("opening a missing linked-worktree path should fall back to the project group key workspace");

    assert_eq!(
        workspace.entity_id(),
        main_workspace_id,
        "missing linked-worktree paths should reuse the main worktree workspace from the project group key"
    );

    multi_workspace.read_with(cx, |mw, cx| {
        assert_eq!(
            mw.workspace().entity_id(),
            main_workspace_id,
            "the active workspace should remain the main worktree workspace"
        );
        assert_eq!(
            PathList::new(&mw.workspace().read(cx).root_paths(cx)),
            project_group_key.path_list().clone(),
            "the activated workspace should use the project group key path list rather than the missing linked-worktree path"
        );
        assert_eq!(
            mw.workspaces().count(),
            1,
            "falling back to the project group key should not create a second workspace"
        );
    });
}

#[gpui::test]
async fn test_remove_keeping_the_project_does_not_switch_projects(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());
    fs.insert_tree("/root_a", json!({ "file.txt": "" })).await;
    fs.insert_tree("/root_b", json!({ "file.txt": "" })).await;
    cx.update(|cx| <dyn Fs>::set_global(fs.clone(), cx));
    let project_a = Project::test(fs.clone(), ["/root_a".as_ref()], cx).await;
    let project_b = Project::test(fs, ["/root_b".as_ref()], cx).await;

    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project_a, window, cx));

    let workspace_a = multi_workspace.read_with(cx, |mw, _cx| mw.workspace().clone());
    let _workspace_b = multi_workspace.update_in(cx, |mw, window, cx| {
        mw.test_add_workspace(project_b, window, cx)
    });
    cx.run_until_parked();

    multi_workspace.update_in(cx, |mw, window, cx| {
        mw.activate(workspace_a.clone(), None, window, cx);
    });
    cx.run_until_parked();

    multi_workspace
        .update_in(cx, |mw, window, cx| {
            mw.remove(
                vec![workspace_a.clone()],
                RemovalIntent::KeepProject,
                window,
                cx,
            )
        })
        .await
        .expect("removing the active workspace should succeed");
    cx.run_until_parked();

    multi_workspace.read_with(cx, |mw, cx| {
        assert_eq!(
            PathList::new(&mw.workspace().read(cx).root_paths(cx)),
            PathList::new(&[PathBuf::from("/root_a")]),
            "the replacement workspace should be in the removed workspace's project"
        );
    });
}

#[gpui::test]
async fn test_find_or_create_local_workspace_reuses_active_workspace_after_sidebar_open(
    cx: &mut TestAppContext,
) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());
    fs.insert_tree("/root_a", json!({ "file.txt": "" })).await;
    let project = Project::test(fs, ["/root_a".as_ref()], cx).await;

    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project, window, cx));

    multi_workspace.update(cx, |mw, cx| {
        mw.open_sidebar(cx);
    });
    cx.run_until_parked();

    let active_workspace = multi_workspace.read_with(cx, |mw, cx| {
        assert_eq!(
            mw.project_groups(cx).len(),
            1,
            "opening the sidebar should retain the active workspace in a project group"
        );
        mw.workspace().clone()
    });
    let active_workspace_id = active_workspace.entity_id();

    let workspace = multi_workspace
        .update_in(cx, |mw, window, cx| {
            mw.find_or_create_local_workspace(
                PathList::new(&[PathBuf::from("/root_a")]),
                None,
                None,
                OpenMode::Activate,
                None,
                window,
                cx,
            )
        })
        .await
        .expect("reopening the same retained local workspace should succeed");

    assert_eq!(
        workspace.entity_id(),
        active_workspace_id,
        "should reuse the retained active workspace after the sidebar is opened"
    );

    multi_workspace.read_with(cx, |mw, _cx| {
        assert_eq!(
            mw.workspaces().count(),
            1,
            "reopening the same retained workspace should not create another workspace"
        );
    });
}

#[gpui::test]
async fn test_close_workspace_opens_unloaded_local_neighbor(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());
    fs.insert_tree("/project-a", json!({})).await;
    fs.insert_tree("/project-b", json!({})).await;
    cx.update(|cx| <dyn Fs>::set_global(fs.clone(), cx));

    let project_a = Project::test(fs.clone(), ["/project-a".as_ref()], cx).await;
    let project_b = Project::test(fs, ["/project-b".as_ref()], cx).await;
    let key_b = project_b.read_with(cx, |project, cx| project.project_group_key(cx));
    let (multi_workspace, cx) = setup_multi_workspace(&[project_a], cx);
    let workspace_a = multi_workspace.read_with(cx, |multi_workspace, _cx| {
        multi_workspace.workspace().clone()
    });

    multi_workspace.update(cx, |multi_workspace, _cx| {
        multi_workspace.test_add_project_group(ProjectGroup {
            key: key_b.clone(),
            workspaces: Vec::new(),
            expanded: true,
        });
    });

    let closed = multi_workspace
        .update_in(cx, |multi_workspace, window, cx| {
            multi_workspace.remove(
                [workspace_a.clone()],
                RemovalIntent::CloseProject,
                window,
                cx,
            )
        })
        .await
        .expect("closing the active workspace should succeed");

    assert!(closed, "close_workspace should remove the active workspace");
    multi_workspace.read_with(cx, |multi_workspace, cx| {
        assert_eq!(
            multi_workspace.workspace().read(cx).project_group_key(cx),
            key_b,
            "the unloaded local neighboring group should be opened"
        );
    });
}

#[gpui::test]
async fn test_remove_project_group_opens_unloaded_local_neighbor(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());
    fs.insert_tree("/project-a", json!({})).await;
    fs.insert_tree("/project-b", json!({})).await;
    cx.update(|cx| <dyn Fs>::set_global(fs.clone(), cx));

    let project_a = Project::test(fs.clone(), ["/project-a".as_ref()], cx).await;
    let project_b = Project::test(fs, ["/project-b".as_ref()], cx).await;
    let key_a = project_a.read_with(cx, |project, cx| project.project_group_key(cx));
    let key_b = project_b.read_with(cx, |project, cx| project.project_group_key(cx));
    let (multi_workspace, cx) = setup_multi_workspace(&[project_a], cx);

    multi_workspace.update(cx, |multi_workspace, _cx| {
        multi_workspace.test_add_project_group(ProjectGroup {
            key: key_b.clone(),
            workspaces: Vec::new(),
            expanded: true,
        });
    });

    let removed = multi_workspace
        .update_in(cx, |multi_workspace, window, cx| {
            multi_workspace.remove_project_group(&key_a, window, cx)
        })
        .await
        .expect("removing the active project group should succeed");

    assert!(
        removed,
        "remove_project_group should remove the active group"
    );

    multi_workspace.read_with(cx, |multi_workspace, cx| {
        assert_eq!(
            multi_workspace.workspace().read(cx).project_group_key(cx),
            key_b,
            "the unloaded local neighboring group should be opened"
        );
    });
}

#[gpui::test]
async fn test_remove_project_group_replaces_unretained_active_workspace(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());
    fs.insert_tree("/project-a", json!({})).await;

    let project_a = Project::test(fs, ["/project-a".as_ref()], cx).await;
    let key_a = project_a.read_with(cx, |project, cx| project.project_group_key(cx));
    let remote_key = ProjectGroupKey::new(
        Some(RemoteConnectionOptions::Mock(
            remote::MockConnectionOptions { id: 1 },
        )),
        PathList::new(&[PathBuf::from("/remote/project")]),
    );
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project_a, window, cx));
    let workspace_a = multi_workspace.read_with(cx, |multi_workspace, _cx| {
        multi_workspace.workspace().clone()
    });

    multi_workspace.update(cx, |multi_workspace, cx| {
        multi_workspace.restore_project_groups(
            vec![
                SerializedProjectGroupState {
                    key: key_a.clone(),
                    expanded: true,
                },
                SerializedProjectGroupState {
                    key: remote_key.clone(),
                    expanded: true,
                },
            ],
            cx,
        );

        assert!(
            !multi_workspace.active_workspace_is_retained(),
            "the active workspace should remain provisional"
        );
        assert_eq!(
            multi_workspace.project_group_keys(),
            vec![key_a.clone(), remote_key.clone()],
            "the remote project group should immediately follow the active local group"
        );
    });

    multi_workspace
        .update_in(cx, |multi_workspace, window, cx| {
            multi_workspace.remove_project_group(&key_a, window, cx)
        })
        .await
        .expect("removing the active project group should succeed");

    multi_workspace.read_with(cx, |multi_workspace, cx| {
        assert_ne!(
            multi_workspace.workspace(),
            &workspace_a,
            "removing the active project group should replace its provisional workspace"
        );
        assert!(
            multi_workspace
                .workspace()
                .read(cx)
                .root_paths(cx)
                .is_empty(),
            "an unloaded remote neighbor should fall back to an empty workspace"
        );
        assert_eq!(
            multi_workspace.project_group_keys(),
            vec![remote_key],
            "only the remote project group should remain"
        );
    });
}

#[gpui::test]
async fn test_remote_project_root_dir_changes_update_groups(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());
    fs.insert_tree("/root_a", json!({ "file.txt": "" })).await;
    fs.insert_tree("/local_b", json!({ "file.txt": "" })).await;
    let project_a = Project::test(fs.clone(), ["/root_a".as_ref()], cx).await;
    let project_b = Project::test(fs.clone(), ["/local_b".as_ref()], cx).await;

    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project_a, window, cx));

    multi_workspace.update(cx, |mw, cx| {
        mw.open_sidebar(cx);
    });
    cx.run_until_parked();

    let workspace_b = multi_workspace.update_in(cx, |mw, window, cx| {
        let workspace = cx.new(|cx| Workspace::test_new(project_b.clone(), window, cx));
        let key = workspace.read(cx).project_group_key(cx);
        mw.activate_provisional_workspace(workspace.clone(), key, window, cx);
        workspace
    });
    cx.run_until_parked();

    multi_workspace.read_with(cx, |mw, _cx| {
        assert_eq!(
            mw.workspace().entity_id(),
            workspace_b.entity_id(),
            "registered workspace should become active"
        );
    });

    let initial_key = project_b.read_with(cx, |p, cx| p.project_group_key(cx));
    multi_workspace.read_with(cx, |mw, _cx| {
        let keys = mw.project_group_keys();
        assert!(
            keys.contains(&initial_key),
            "project groups should contain the initial key for the registered workspace"
        );
    });

    let remote_worktree = project_b.update(cx, |project, cx| {
        project.add_test_remote_worktree("/remote/project", cx)
    });
    cx.run_until_parked();

    let worktree_id = remote_worktree.read_with(cx, |wt, _| wt.id().to_proto());
    remote_worktree.update(cx, |worktree, _cx| {
        worktree
            .as_remote()
            .unwrap()
            .update_from_remote(proto::UpdateWorktree {
                project_id: 0,
                worktree_id,
                abs_path: "/remote/project".to_string(),
                root_name: "project".to_string(),
                updated_entries: vec![proto::Entry {
                    id: 1,
                    is_dir: true,
                    path: "".to_string(),
                    inode: 1,
                    mtime: Some(proto::Timestamp {
                        seconds: 0,
                        nanos: 0,
                    }),
                    is_ignored: false,
                    is_hidden: false,
                    is_external: false,
                    is_fifo: false,
                    size: None,
                    canonical_path: None,
                    is_unloaded: false,
                }],
                removed_entries: vec![],
                scan_id: 1,
                is_last_update: true,
                updated_repositories: vec![],
                removed_repositories: vec![],
                root_repo_common_dir: None,
                root_repo_is_linked_worktree: false,
            });
    });
    cx.run_until_parked();

    let updated_key = project_b.read_with(cx, |p, cx| p.project_group_key(cx));
    assert_ne!(
        initial_key, updated_key,
        "remote worktree update should change the project group key"
    );

    multi_workspace.read_with(cx, |mw, _cx| {
        let keys = mw.project_group_keys();
        assert!(
            keys.contains(&updated_key),
            "project groups should contain the updated key after remote change; got {keys:?}"
        );
        assert!(
            !keys.contains(&initial_key),
            "project groups should no longer contain the stale initial key; got {keys:?}"
        );
    });
}

#[gpui::test]
async fn test_close_workspace_with_remote_neighbor_does_not_create_local_workspace(
    cx: &mut TestAppContext,
) {
    // Regression test: closing a workspace whose neighboring group is
    // remote with no existing workspace should not create a local
    // workspace with the remote paths.
    init_test(cx);
    let fs = FakeFs::new(cx.executor());
    fs.insert_tree("/root_a", json!({ "file.txt": "" })).await;
    let project_a = Project::test(fs, ["/root_a".as_ref()], cx).await;

    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project_a, window, cx));

    multi_workspace.update(cx, |mw, cx| {
        mw.open_sidebar(cx);
    });
    cx.run_until_parked();

    // Add a mock-remote group with no workspace as the second group.
    let remote_key = ProjectGroupKey::new(
        Some(RemoteConnectionOptions::Mock(
            remote::MockConnectionOptions { id: 1 },
        )),
        PathList::new(&[PathBuf::from("/remote/project")]),
    );
    multi_workspace.update(cx, |mw, _cx| {
        mw.test_add_project_group(ProjectGroup {
            key: remote_key.clone(),
            workspaces: Vec::new(),
            expanded: true,
        });
    });

    let workspace_a = multi_workspace.read_with(cx, |mw, _cx| mw.workspace().clone());

    // Close workspace A. The neighbor is the remote group with no workspace.
    // The fix should skip find_or_create_local_workspace and fall through
    // to creating an empty workspace instead.
    multi_workspace
        .update_in(cx, |mw, window, cx| {
            mw.remove(
                [workspace_a.clone()],
                RemovalIntent::CloseProject,
                window,
                cx,
            )
        })
        .await
        .expect("close_workspace should succeed");

    cx.run_until_parked();

    multi_workspace.update(cx, |mw, cx| {
        // The active workspace should NOT be a local workspace with the
        // remote paths. It should be an empty workspace (no worktrees).
        let workspaces: Vec<_> = mw.workspaces().cloned().collect();
        for ws in &workspaces {
            let key = ws.read(cx).project_group_key(cx);
            assert!(
                key.host().is_some()
                    || key.path_list().paths() != [PathBuf::from("/remote/project")],
                "remote neighbor should not have created a local workspace"
            );
        }
    });
}

#[gpui::test]
async fn test_remove_project_group_with_remote_neighbor_does_not_create_local_workspace(
    cx: &mut TestAppContext,
) {
    // Regression test: removing a project group whose neighboring group is
    // remote with no workspace should not create a local workspace with
    // the remote paths.
    init_test(cx);
    let fs = FakeFs::new(cx.executor());
    fs.insert_tree("/root_a", json!({ "file.txt": "" })).await;
    let project_a = Project::test(fs, ["/root_a".as_ref()], cx).await;

    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project_a.clone(), window, cx));

    multi_workspace.update(cx, |mw, cx| {
        mw.open_sidebar(cx);
    });
    cx.run_until_parked();

    let key_a = project_a.read_with(cx, |p, cx| p.project_group_key(cx));

    // Add a mock-remote group with no workspace.
    let remote_key = ProjectGroupKey::new(
        Some(RemoteConnectionOptions::Mock(
            remote::MockConnectionOptions { id: 1 },
        )),
        PathList::new(&[PathBuf::from("/remote/project")]),
    );
    multi_workspace.update(cx, |mw, _cx| {
        mw.test_add_project_group(ProjectGroup {
            key: remote_key.clone(),
            workspaces: Vec::new(),
            expanded: true,
        });
    });

    // Remove the local group A. The neighbor is the remote group with no
    // workspace. The fix should skip find_or_create_local_workspace and
    // fall through to creating an empty workspace.
    multi_workspace
        .update_in(cx, |mw, window, cx| {
            mw.remove_project_group(&key_a, window, cx)
        })
        .await
        .expect("remove_project_group should succeed");

    cx.run_until_parked();

    multi_workspace.update(cx, |mw, cx| {
        let workspaces: Vec<_> = mw.workspaces().cloned().collect();
        for ws in &workspaces {
            let key = ws.read(cx).project_group_key(cx);
            assert!(
                key.host().is_some() || key.path_list().paths() != [PathBuf::from("/remote/project")],
                "remote neighbor should not have created a local workspace after remove_project_group"
            );
        }
    });
}
