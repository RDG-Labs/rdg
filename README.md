# Rdg

<div align="center">

**A local-first code editor for people who live in the terminal.**

[![Latest release](https://img.shields.io/github/v/release/RDG-Labs/rdg?display_name=tag&sort=semver)](https://github.com/RDG-Labs/rdg/releases)
[![Build](https://img.shields.io/github/actions/workflow/status/RDG-Labs/rdg/ci.yml?branch=main&label=build)](https://github.com/RDG-Labs/rdg/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-GPL--3.0%20%2F%20Apache--2.0-blue)](#licensing)
[![Roadmap](https://img.shields.io/badge/roadmap-read-7c3aed)](./product/ROADMAP.md)

</div>

Rdg is a fork of [Zed](https://github.com/zed-industries/zed) shaped around one
idea: the editor and the terminal should be one workspace. Open multiple shells
as a tiled terminal group, run services and coding CLIs side by side, and keep
editing in the same project.

## What makes Rdg different

- **Terminal-first.** Tiled terminal groups are regular workspace items beside
  editor tabs, sharing the project, file explorer, keymap, theme, and tasks.
- **Local-first.** No account, telemetry, AI service, or multiplayer backend is
  required. Rdg starts and works without contacting a server.
- **Open-source foundation.** The editor core, LSP, extensions, debugger, and
  Git integration come from Zed's strong foundation.

[Read the roadmap](./product/ROADMAP.md) · [View the project board](https://github.com/orgs/RDG-Labs/projects/1) · [Open an issue](https://github.com/RDG-Labs/rdg/issues/new/choose) · [Read terminal docs](./docs/src/terminal.md)

## Relationship to Zed

Rdg is a modified fork of Zed, which is copyright Zed Industries, Inc. and licensed under
GPL-3.0-or-later with Apache-2.0 components. Rdg is not affiliated with, endorsed by, or
supported by Zed Industries. Please do not report Rdg issues to the upstream project.

The fork's divergence from upstream is documented patch by patch in [patches.md](./patches.md).

## Install or build

Published release artifacts are available on the [Releases page](https://github.com/RDG-Labs/rdg/releases).
For development or unsupported platforms, build from source:

- [Building Rdg for macOS](./docs/src/development/macos.md)
- [Building Rdg for Linux](./docs/src/development/linux.md)
- [Building Rdg for Windows](./docs/src/development/windows.md)
- [Keyboard shortcuts and keymaps](./SHORTCUT.md)

Use `./script/clippy` rather than `cargo clippy` — it applies the workspace lint configuration.

## External CLI orchestration

Rdg Terminal Groups can host and orchestrate external coding CLIs without embedding an
agent or sending telemetry. Install the RDG orchestration skill interactively:

```bash
npx skills add RDG-Labs/rdg --skill rdg-orchestration
```

`npx skills` lets you choose project or global scope, supported agents, and symlink or
copy installation. The skill teaches workers how to spawn children, send tasks, watch
structured events, report status, and coordinate recursive Terminal Group workers.

You can also use **+ → Install RDG Orchestration Skill…** from a Terminal Group. Rdg opens
the visible interactive install command in a terminal rather than running `npx` invisibly.

## Roadmap and contributing

See the [roadmap](./product/ROADMAP.md) for product direction and
[CONTRIBUTING.md](./CONTRIBUTING.md) for development guidelines. Bug reports,
small fixes, and documentation improvements are welcome.

## Licensing

Rdg inherits Zed's licensing and cannot be relicensed:

- Most crates are **GPL-3.0-or-later** — see [LICENSE-GPL](./LICENSE-GPL).
- The framework and primitive crates (`gpui*`, `util`, `http_client`, `sum_tree`,
  `collections`, and others) are **Apache-2.0** — see [LICENSE-APACHE](./LICENSE-APACHE).

Each crate carries a symlink to the license that governs it. Upstream copyright notices must
be preserved when modifying files; add your own notice alongside them rather than replacing
them.

License information for third party dependencies must be correctly provided for CI to pass.

We use [`cargo-about`](https://github.com/EmbarkStudios/cargo-about) to automatically comply with open source licenses. If CI is failing, check the following:

- Is it showing a `no license specified` error for a crate you've created? If so, add `publish = false` under `[package]` in your crate's Cargo.toml.
- Is the error `failed to satisfy license requirements` for a dependency? If so, first determine what license the project has and whether this system is sufficient to comply with this license's requirements. If you're unsure, ask a lawyer. Once you've verified that this system is acceptable add the license's SPDX identifier to the `accepted` array in `script/licenses/zed-licenses.toml`.
- Is `cargo-about` unable to find the license for a dependency? If so, add a clarification field at the end of `script/licenses/zed-licenses.toml`, as specified in the [cargo-about book](https://embarkstudios.github.io/cargo-about/cli/generate/config.html#crate-configuration).
