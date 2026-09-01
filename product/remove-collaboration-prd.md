# PRD — Remove multiplayer collaboration

| | |
|---|---|
| **Status** | Approved — all five collaboration crates confirmed for removal |
| **Owner** | Irfan Saf (product), TBD (eng) |
| **Date** | 2026-09-01 |
| **Target** | Rdg (Zed fork) |
| **Prompted by** | A prebuilt WebRTC download failing mid-build and aborting a release bundle |

---

## 1. Why

Rdg does not use multiplayer. The editor has already dropped AI and the remote development daemon; calls, shared projects, and channels are the remaining piece of upstream Zed that this fork carries but does not run.

The immediate trigger is concrete. Building the app downloads a **prebuilt WebRTC binary** from `zed-industries/livekit-rust-sdks` releases:

```
error: failed to run custom build command for `webrtc-sys`
  Failed to send HTTP request to download WebRTC
  url: .../webrtc-0001d84-4/webrtc-mac-x64-release.zip
  operation timed out
```

That is a **network dependency in the middle of an offline-capable Rust build**, and it aborted a release bundle today. Removing collaboration makes builds deterministic.

### 1.1 Is it already broken?

Partly — and this PRD is deliberately careful not to overclaim, because "provably non-functional" was the decisive argument for removing the remote daemon and it does **not** transfer cleanly here.

**Unreachable through the UI.** `7fd594a` (#3) deleted the `authenticate` function from `main.rs` and removed the account UI. The app never signs in, and calls require an authenticated user on a collab server.

**But not provably dead.** A `client::SignIn` action is still registered (`client.rs:170`) and is dispatchable from the command palette. Whether it completes against upstream's servers with account connectivity stripped is **untested**. Unlike the remote daemon — whose download URL pointed at an asset this fork will never publish — there is no proof here that the path is impossible.

So the case rests on **"we do not use it"**, which is sufficient on its own, plus the build determinism win. It does not rest on the feature being broken.

**Decision (2026-09-01):** all five crates — `call`, `collab_ui`, `collab`, `livekit_api`, `livekit_client` — are confirmed for removal. `title_bar` stays; only its `collab.rs` comes out.

### 1.2 What it costs to keep

| Cost | Measured |
|---|---|
| Build determinism | A large prebuilt blob fetched over the network mid-build; already caused one failed release |
| Build time | **Compiled twice per bundle** — see below |
| Code | 60,506 lines across six crates |
| Product | A collab panel, screen-share controls, and a `SignIn` action that lead nowhere |

**The livekit stack is built twice in every bundle.** `script/bundle-mac` runs two cargo
invocations: the build step, and `cargo bundle`. The build step passes
`--config .cargo/bundle-config.toml`, which sets `-Z share-generics=y`; `cargo bundle` does
not, and cannot — it shells out to its own `cargo build` subprocess, which does not inherit
`--config`. The two therefore have different rustflags, different fingerprints, and separate
artifact caches.

Measured on the 2026-09-01 bundle: `webrtc-sys` and `webrtc-sys-build` each appear twice in
the log (lines 323/333 and 877), once per bucket. So whatever livekit costs to compile,
removing it saves roughly double that per bundle.

This also means the bundle script's own double-compile is worth fixing independently —
plausibly by exporting `RUSTFLAGS` and `RUSTC_BOOTSTRAP` as environment variables, which do
propagate to subprocesses. That is a separate change from this PRD, and untested.

---

## 2. The dependency shape

This is where the previous removal PRD went wrong: it called a phase "remove the entry points" when the code was entangled. Measured this time.

| Crate | Lines | Depended on by | Shape |
|---|---|---|---|
| **`collab`** | **42,892** | **nothing** — `rdg` does not link it | A **server binary** (`[[bin]] name = "collab"`). Like `remote_server`, a leaf |
| `livekit_api` | 497 | `livekit_client`, `collab` | The WebRTC layer |
| `livekit_client` | 4,248 | 3 crates | Pulls `libwebrtc` — the blob download lives here |
| `call` | 3,407 | 5: `collab`, `collab_ui`, `git_ui`, `rdg`, `title_bar` | Session state |
| `collab_ui` | 6,797 | 2 | The collab panel |
| `title_bar` | 2,665 | 3 | **Load-bearing — must stay.** Only `collab.rs` (783 lines) is collab |

Two things follow.

**`collab` is a free win.** 42,892 lines, a server this fork will never run, and *nothing depends on it*. It is exactly the shape `remote_server` was.

**`title_bar` must not be deleted.** It is the application title bar. Only its `collab.rs` comes out. Any plan that says "remove title_bar" is wrong.

**`git_ui` is the awkward edge.** It reads `call::ActiveCall` to attribute commits to call participants (`git_panel.rs:4393`, `4430`, `8487`) — co-authored commits during a pairing session. That is a real feature touching a crate we are keeping, and it must be handled deliberately rather than stubbed.

---

## 3. Phases

Ordered by risk, each independently shippable and revertible.

**Correction (2026-09-01):** an earlier draft claimed *"Phase 1 alone delivers the
build-determinism win."* That was wrong. The WebRTC blob is pulled by `livekit_client`, not by
`collab`, and `webrtc-sys` is still in `Cargo.lock` after Phase 1 shipped. Phase 1 delivered a
build-*time* win — 42,892 lines gone and CI no longer compiling a server binary on every push —
but the mid-build network fetch survives until **Phase 3**. The trigger for this work is not
resolved until then.

### Phase 1 — Remove the collab server crate

Delete `crates/collab`. Nothing depends on it.

**Exit:** `cargo check --workspace --all-targets` clean; workspace 42,892 lines lighter.

### Phase 2 — Remove the collaboration UI

**Reordered 2026-09-01.** This was Phase 3. The original order was wrong: it deleted
`livekit_client` and `call` before `collab_ui` and `title_bar/collab.rs`, which are their
*consumers*. Removing a supplier before its consumers leaves the workspace uncompilable, so
the UI comes out first.

Delete `crates/collab_ui` (6,797 lines) and `crates/title_bar/src/collab.rs` (783). Remove the
`CollabPanel` registration and `ToggleFocus` action in `rdg.rs`, the `collab_ui::init` call in
`main.rs`, and `render_collaborator_list` from `title_bar.rs`.

`collab_ui` is a leaf — only `rdg` depends on it. `title_bar` **stays**; only its `collab.rs`
comes out.

**`git_ui` costs one line.** Every `call::` use in `git_panel.rs` already sits behind
`#[cfg(feature = "call")]` with a `#[cfg(not(feature = "call"))]` fallback that exists and
compiles. Dropping `features = ["call"]` from `rdg/Cargo.toml:82` removes call-participant
commit attribution; commits are attributed to the local author only. No new code.

**Exit:** no UI path reaches collaboration; `call` and the livekit crates are orphaned,
depended on by nothing.

### Phase 3 — Remove livekit, `call`, and the WebRTC download

Delete `crates/call` (3,407), `crates/livekit_client` (4,248) and `crates/livekit_api` (497),
now that Phase 2 has orphaned them. Remove the `libwebrtc`/`livekit` workspace dependencies.

This is the phase that **kills the mid-build network fetch**.

Stale livekit paths appear as fixture strings in `file_finder_tests.rs` (lines 5066–5097).
They are test data, not references — they compile either way. Refresh them or leave them.

**Exit:** no build script downloads anything; a clean checkout builds offline.

---

## 4. Risks

| Risk | Mitigation |
|---|---|
| `title_bar` is deleted by mistake | Explicit: only `collab.rs` is in scope. The crate stays |
| `git_ui` commit attribution silently disappears | Resolved by decision, not left to a later phase. It is removed deliberately, and noted here so its absence is not later mistaken for a regression |
| `call` is more entangled than 3,407 lines suggests | Phase 2 removes its UI consumers first, so by Phase 3 `call` should be orphaned. If it still has live callers then, that is the signal the estimate was wrong — stop and re-measure rather than forcing it |
| Stopping after Phase 2 leaves the trigger unfixed | Explicit: the WebRTC fetch dies in Phase 3 and nowhere earlier. Phases 1 and 2 are code-size and reachability wins only |
| Upstream merge divergence grows | Deleting whole crates conflicts more cleanly than editing them: "upstream changed a file we deleted" resolves once |
| Scope is underestimated again | Each phase re-measures before starting. Phase boundaries are where estimates get corrected, not where they get defended |

---

## 5. Success criteria

1. `cargo check --workspace --all-targets` clean after each phase.
2. **No build step performs a network download.** Verifiable by building with networking disabled.
3. Bundle time measured before and after.
4. No UI path reaches a call, channel, or shared project.
5. `git_ui`'s attribution behaviour is a written decision, not an accident.

---

## 6. Open questions

| # | Question | Blocks |
|---|---|---|
| Q1 | Does `client::SignIn` still complete against upstream servers? Determines whether §1.1 can be strengthened from "unused" to "non-functional" | Nothing — the case does not depend on it |
| ~~Q2~~ | ~~Keep call-participant commit attribution in `git_ui`?~~ **Resolved: no.** `call` is removed outright, so the attribution goes with it | — |
| Q3 | `client` and `user_store` survive this removal. Are they still needed once collaboration is gone, or is that a fourth phase? | Post-Phase 3 |
