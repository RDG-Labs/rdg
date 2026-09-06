use anyhow::{Result, anyhow};
use collections::{BTreeMap, IndexSet};
use gpui::{App, Context, Entity, Task};
use language::{LanguageName, LanguageRegistry, Toolchain, ToolchainMetadata, ToolchainScope};
use std::{path::PathBuf, sync::Arc};
use util::maybe;

use crate::{Project, ProjectPath, toolchain_store::ToolchainStore};

impl Project {
    pub async fn toolchain_metadata(
        languages: Arc<LanguageRegistry>,
        language_name: LanguageName,
    ) -> Option<ToolchainMetadata> {
        languages
            .language_for_name(language_name.as_ref())
            .await
            .ok()?
            .toolchain_lister()
            .map(|lister| lister.meta())
    }

    pub fn add_toolchain(
        &self,
        toolchain: Toolchain,
        scope: ToolchainScope,
        cx: &mut Context<Self>,
    ) {
        maybe!({
            self.toolchain_store.as_ref()?.update(cx, |this, cx| {
                this.add_toolchain(toolchain, scope, cx);
            });
            Some(())
        });
    }

    pub fn remove_toolchain(
        &self,
        toolchain: Toolchain,
        scope: ToolchainScope,
        cx: &mut Context<Self>,
    ) {
        maybe!({
            self.toolchain_store.as_ref()?.update(cx, |this, cx| {
                this.remove_toolchain(toolchain, scope, cx);
            });
            Some(())
        });
    }

    pub fn user_toolchains(
        &self,
        cx: &App,
    ) -> Option<BTreeMap<ToolchainScope, IndexSet<Toolchain>>> {
        Some(self.toolchain_store.as_ref()?.read(cx).user_toolchains())
    }

    pub fn resolve_toolchain(
        &self,
        path: PathBuf,
        language_name: LanguageName,
        cx: &App,
    ) -> Task<Result<Toolchain>> {
        if let Some(toolchain_store) = self.toolchain_store.as_ref().map(Entity::downgrade) {
            cx.spawn(async move |cx| {
                toolchain_store
                    .update(cx, |this, cx| {
                        this.resolve_toolchain(path, language_name, cx)
                    })?
                    .await
            })
        } else {
            Task::ready(Err(anyhow!("This project does not support toolchains")))
        }
    }

    pub fn toolchain_store(&self) -> Option<Entity<ToolchainStore>> {
        self.toolchain_store.clone()
    }

    pub fn activate_toolchain(
        &self,
        path: ProjectPath,
        toolchain: Toolchain,
        cx: &mut App,
    ) -> Task<Option<()>> {
        let Some(toolchain_store) = self.toolchain_store.clone() else {
            return Task::ready(None);
        };
        toolchain_store.update(cx, |this, cx| this.activate_toolchain(path, toolchain, cx))
    }

    pub fn active_toolchain(
        &self,
        path: ProjectPath,
        language_name: LanguageName,
        cx: &App,
    ) -> Task<Option<Toolchain>> {
        let Some(toolchain_store) = self.toolchain_store.clone() else {
            return Task::ready(None);
        };
        toolchain_store
            .read(cx)
            .active_toolchain(path, language_name, cx)
    }
}