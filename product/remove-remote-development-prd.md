# PRD — Remove remote development

| | |
|---|---|
| **Status** | Draft for review |
| **Owner** | Irfan Saf (product), TBD (eng) |
| **Date** | 2026-09-01 |
| **Target** | Rdg (Zed fork) |
| **Prompted by** | `remote_server` costing 13m 27s of every macOS bundle |

---

## 1. The question is not the one we started with

The ask was "remove `remote_server`, we don't need it in a fork." Exploration says the honest framing is different:

> **Remote development is already broken in Rdg.** The UI still offers it, and it cannot work.

That changes the decision from *"should we drop a working feature to save build time"* to *"we are shipping a visibly broken feature — remove it or fix it."* Build time is then a bonus, not the argument.

### 1.1 Why it cannot work today

Connecting to a remote host requires the client to install a matching `remote_server` binary on that host. The client obtains it like this:

```
RemoteConnection::download_server_binary_locally
  └─ AutoUpdater::download_remote_server_release
       └─ AutoUpdater::get_release_asset(..., asset = "zed-remote-server", ...)
            └─ queries the release API through `client`, gated on `client.telemetry()`
```

Three independent reasons that chain is dead in this fork:

1. **Account connectivity and telemetry were disabled** in `7fd594a` (#3), and `get_release_asset` runs through that same client.
2. The asset it asks for is **`zed-remote-server`** — upstream Zed's artifact name, from upstream's release infrastructure.
3. **RDG-Labs publishes no such asset.** The one release cut so far contained a single `.dmg`.

So a user who opens the remote UI and tries to connect gets a failure, not a feature.

### 1.2 What it costs to keep

| Cost | Measured |
|---|---|
| Build time | **13m 27s** per macOS bundle — a dedicated second `cargo build` invocation, ~35% of total bundle time |
| Code | `remote_server` 7,718 lines; `remote` 7,439; `remote_connection` 853 |
| Artifact | The bundle gzips `zed-remote-server-macos-*.gz` on every run, which nothing consumes |
| Product | A menu entry that leads to a dead end |

---

## 2. Two separable things

Conflating these is the main risk in this work.

| | Lines | Depended on by | Shape |
|---|---|---|---|
| **`remote_server`** — the daemon that runs on the remote host | 7,718 | 4 crates, **all as `[dev-dependencies]`** | Standalone `[[bin]]` plus a test-support lib |
| **`remote`** — the client half | 7,439 | **16 crates**, including `project` and `workspace`, as real dependencies | Woven through core |

`remote_server` is a **leaf**. Every one of `collab`, `rdg`, `recent_projects`, and `sidebar` depends on it only under `[dev-dependencies]`, for `HeadlessProject` in tests. Nothing in the shipping binary links it.

`remote` is not a leaf. Removing it is surgery on `project` and `workspace` — the same files the terminal workspace work deliberately kept its hands off.

**These must be phased separately.** Removing the daemon is a contained change; removing the client is not.

---

## 3. Options

| | **A. Daemon only** | **B. Whole stack** ✅ | **C. Fix it** |
|---|---|---|---|
| Removes | `remote_server` | daemon + client + UI | nothing |
| Build time saved | ~13m 27s/bundle | ~13m 27s + client compile | none |
| Leaves broken UI | **Yes** | No | No |
| Scope | Leaf, contained | 16 crates incl. `project`, `workspace` | Publish assets, repoint auto-update, own the protocol |
| Verdict | Worst of both — keeps the dead end while removing the thing that could have fixed it | Coherent | Real feature work for a fork with no server infrastructure |

**Chosen: B, phased.** Option A is explicitly rejected: it makes the product *less* fixable while leaving the user-visible failure in place. Option C means committing to publishing and versioning a server binary per platform, matching client and server versions, and maintaining a wire protocol — for a local-first editor fork, that is a large ongoing cost with no current demand.

---

## 4. Phasing

Ordered so each phase is independently shippable and independently revertible.

### Phase 1 — Close the user-visible dead end

Remove the entry points that lead nowhere: the `OpenRemote` action, `RemoteServerProjects` UI in `recent_projects`, and the sidebar's remote entry.

Nothing else changes. If we later choose Option C, this is the cheapest phase to undo.

**Exit:** no path in the UI reaches remote connection.

### Phase 2 — Remove the daemon

Delete `crates/remote_server`. Drop the four `[dev-dependencies]` and the tests that construct `HeadlessProject`. Remove the second `cargo build` invocation and the `zed-remote-server-*.gz` step from `script/bundle-mac`, plus the equivalents in the other platform bundle scripts and CI.

Contained because the daemon is a leaf — no shipping code links it.

**Exit:** bundles no longer build or emit a server binary; `cargo test --workspace` passes.

### Phase 3 — Remove the client, if Phases 1–2 hold

Only after the first two have been in use. Remove `remote`, `remote_connection`, and the `is_via_remote_server` branches through `project` and `workspace`.

This is the phase that touches core files, so it wants its own review and its own change budget, in the spirit of the ≤150-line ceiling the terminal workspace work held itself to.

**Exit:** no remote code remains; core files reviewed on their own merits.

---

## 5. Risks

| Risk | Mitigation |
|---|---|
| `remote` turns out to carry non-SSH responsibilities that `project`/`workspace` rely on | Phase 3 is gated on discovery, not scheduled. If it is entangled, stop after Phase 2 — most of the value is already banked |
| Divergence from upstream Zed grows, making merges harder | Real, and the honest trade. Deleting whole crates is *cleaner* to merge than editing them: conflicts appear as "upstream changed a file we deleted", which resolves once |
| Someone later wants remote development | Phase 1 is trivially revertible; Phases 2–3 are recoverable from git history or by re-vendoring upstream's crates |
| Dev-container support shares plumbing with remoting | `OpenDevContainer` sits beside `OpenRemote` in `recent_projects`. Confirm before Phase 1 whether it rides the same connection layer; if so it is in scope for the same decision |

---

## 6. Success criteria

1. No UI path reaches a remote-connection flow.
2. `script/bundle-mac` no longer builds `remote_server`; bundle time drops by roughly 13 minutes, measured before and after.
3. `cargo test --workspace` passes with no remote tests remaining.
4. No `zed-remote-server` artifact is produced.
5. `crates/workspace` and `crates/project` changes in Phase 3 are reviewed as a separate commit, not folded into deletions.

---

## 7. Open questions

| # | Question | Blocks |
|---|---|---|
| Q1 | Does `OpenDevContainer` share the remote connection layer? If so, is dev-container support also out of scope for this fork? | Phase 1 |
| Q2 | Keep `HeadlessProject` for headless testing even without shipping a daemon? Some workspace tests may find it useful independent of remoting | Phase 2 |
| Q3 | `auto_update` still initializes in `main.rs:630` against disabled connectivity. Is auto-update itself dead weight, and should it be a separate removal? | — |
