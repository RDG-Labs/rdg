> [!IMPORTANT]
> Remove this line to confirm you've reviewed this PR before submitting.

# Rdg

Rdg is a code editor built around a tiled terminal workspace. It is a fork of
[Zed](https://github.com/zed-industries/zed), reshaped around three decisions:

- **Terminals are first-class.** A directly manipulable tiled terminal grid lives next to
  your file tabs, sharing the project, file explorer, keymap, theme, and task system —
  rather than being confined to a dock strip. See
  [the terminal workspace PRD](./product/terminal-workspace-prd.md).
- **No AI.** The agent panel, model providers, edit prediction, and related settings are
  removed. Rdg does not talk to a language model.
- **No account, no telemetry.** Sign-in and telemetry initialization are removed; Rdg starts
  and runs without contacting a server.

Everything else Zed does well — the editor core, LSP, extensions, debugger, Git integration —
is intact.

## Relationship to Zed

Rdg is a modified fork of Zed, which is copyright Zed Industries, Inc. and licensed under
GPL-3.0-or-later with Apache-2.0 components. Rdg is not affiliated with, endorsed by, or
supported by Zed Industries. Please do not report Rdg issues to the upstream project.

The fork's divergence from upstream is documented patch by patch in [patches.md](./patches.md).

## Building Rdg

There are no prebuilt binaries yet; build from source.

- [Building Rdg for macOS](./docs/src/development/macos.md)
- [Building Rdg for Linux](./docs/src/development/linux.md)
- [Building Rdg for Windows](./docs/src/development/windows.md)

Use `./script/clippy` rather than `cargo clippy` — it applies the workspace lint configuration.

## Contributing

See [CONTRIBUTING.md](./CONTRIBUTING.md).

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
