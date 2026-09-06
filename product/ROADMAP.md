# Rdg roadmap

Rdg is an open-source, terminal-first code editor. The roadmap is a product
commitment about direction, not a promise that every item will ship on a date.
Small fixes and user feedback can change the order.

**Last reviewed:** 2026-09-09 · [View the public project board](https://github.com/orgs/RDG-Labs/projects/1)

## Product direction

> Make the editor the best place to run, observe, and control the work around a
> codebase — without accounts, telemetry, AI services, or a collaboration
> backend.

That gives Rdg a clear boundary: local-first, terminal-native, fast to build,
and easy to understand.

## Status

### Shipped

- **Terminal groups** — tiled terminals are a first-class workspace item beside
  editor tabs.
- **External CLI orchestration** — terminal groups can host visible coding CLI
  workers and coordinate them through the RDG orchestration skill.
- **Local-first defaults** — account connectivity, telemetry, and AI surfaces
  are removed.
- **Smaller, deterministic builds** — the unused collaboration stack, WebRTC
  download, and remote-development UI have been removed.
- **Release pipeline** — versioned macOS, Linux, and Windows release workflows
  are in place, with v0.2.0 published.

### Now — v0.3.0: reliable terminal workspaces and local agent control

Target: a group can host **more agents than it can visibly fit**, while the
visible grid stays within the measured performance budget. The cap is a
performance backstop, not a ceiling on how many agents can work.

**Phase 0 — Measure the real ceiling first.** The current `max_tiles` default of
32 is a guess. Record production-shaped benchmarks (per PRD §8.1) for tile
counts 12, 24, and 40, for both idle and streaming workloads, before deciding
where overflow begins. Publish the numbers; the overflow threshold is
evidence-based, not arbitrary.

First pass measured (debug build, `crates/terminal_group_benchmarks`,
`test-support`-free, `bench-support` seam):

| Tiles | Split growth (grow 1→N) | Frame cost (repaint N) |
| ----- | ------------------------ | ---------------------- |
| 12    | ~51 µs                   | ~15.7 ms               |
| 24    | ~180 µs                  | ~33.2 ms               |

Readings:

- **Split growth is superlinear** — 2× the panes (12→24) costs ~3.5× the
  reshape time (51→180 µs). Tree mutation / `bounding_boxes` maintenance of a
  deep grid grows faster than the pane count.
- **Frame cost is near-linear** — 2× the visible tiles (12→24) costs ~2.1× the
  per-frame repaint (15.7→33.2 ms), consistent with all tiles repainting
  together. This is the visible-grid budget ceiling.

Implication for overflow: the split-reshape cost (superlinear) is the tighter
constraint on **visible** tile count, while per-agent work can keep growing
unbounded once it moves to overflow rows. Set the visible-grid cap from the
**frame** curve; move work to overflow before reshape cost grows faster than
visible tiles can be usefully read.

Still open: absolute numbers must be re-measured under an optimized
`release-fast` build (the full workspace compile exceeded this session's
tooling timeout). The scaling ratios above hold; the absolute µs/ms ceiling for
the frame budget does not yet. Streaming (4-of-12 ≈ 1000 lines/s) and 40-tile
runs are gated behind `ZED_BENCH_HUGE` and remain to be captured.

**Phase 1 — Harden the terminal group.**

- Fix focus, move, close, resize, and tab-boundary edge cases.
- Preserve layout safely across restart and workspace changes.
- Keep the one-terminal-per-tile invariant intact under mouse, keyboard, task,
  and restore paths.
- Turn the terminal workspace PRD into a maintained, testable specification.

**Phase 2 — Worker overflow.** Decouple "how many agents are running" from
"how many terminal tiles are visible".

Shipped (Increment A):

- Spawning a worker or agent at the visible-tile cap **no longer refuses**; it
  spills into an overflow set, each holding a real attached PTY (standalone
  `TerminalView`, no `Pane`), with a stable worker id (`next_overflow_id`),
  full metadata, and `WorkerEvent::Spawned/Updated/Closed`.
- Overflow workers appear in **Mission Control** and a new on-grid **overflow
  strip** (status + close), and the control plane (`send`/`restart`/`close`)
  resolves them by id.
- A dedicated `worker_init_command` shared with visible workers keeps the RDG
  control protocol consistent (`RDG_GROUP_ID`/`RDG_WORKER_ID`/…).
- Test: `test_worker_spawn_overflows_past_the_visible_cap`.

Next (Increment B):

- Promote an overflow worker to a visible tile **preserving the live process**
  (its terminal is re-homed into a new tile). Shipped: focus/"promote" from the
  overflow strip or Mission Control grows the grid, re-using the same PTY.
- Demote a visible tile back to overflow is **shipped**: "Send to overflow"
  from the tile header or Mission Control re-homes the tile's `Terminal`/PTY
  into the overflow set under the same stable id. Retaining the `Entity<Terminal>`
  keeps the process alive while the tile's view is dropped, so the earlier
  "architecturally risky" concern did not materialize.
- Worker lookup stays O(1) by `worker_id` (already keyed by hash map, never a
  tree walk).
- Pause/resume a worker from a row without hunting its tile. **Shipped**: the
  tile header, overflow strip, and Mission Control pause/resume a worker
  (SIGSTOP/SIGCONT on the foreground process group), whether on-grid or
  overflowed.
- Restore the overflow set on restart within the same layout guarantees.
  **Shipped**: overflow workers' commands are serialized with the layout and
  re-spawned on restore (live PTYs can't survive a restart without a daemon;
  the set and its commands are preserved).

  The `Worker` object is the seam where Phase 4's headless/background workers
  slot in later — never headless in v0.3, always a real attached PTY.

**Phase 3 — Performance refinements, measured against overflow.**

- Repaint tiering, PTY resize coalescing, and deferred spawn (PRD §8.2–8.4)
  re-measured now that a group can hold many more workers than visible tiles.
- Record the numbers for the overflow state specifically (many workers, few
  visible tiles).

**Phase 4 — Release confidence.**

- Smoke-test the packaged app and `rdg` CLI on each supported platform.
- Document install, upgrade, rollback, and troubleshooting paths.
- Keep release artifacts and version behavior predictable for contributors.
- The golden workflow: open repo → create group → start service + agent →
  split/rearrange → send follow-up → observe a failed worker → recover → restart
  → restore layout. Works on macOS, Linux, Windows.

**Throughout — healthy OSS feedback loop.**

- Keep issue forms Rdg-specific and easy to complete.
- Triage reproducible bugs before adding new surface area.
- Use roadmap issues for work that has an owner, a success condition, and a
  reason to exist.

### v0.3.0 scope and ship gates

Still a v0.x product: one coherent story, not a sprawling feature dump. The
whole release is the theme **“run as many local agents as a project needs,
without losing your workspace or losing track of the work.”**

Ship gate — all must pass before tagging v0.3.0:

| Area              | Gate                                                                    |
| ----------------- | ----------------------------------------------------------------------- |
| Measure | Split 12→24 ~51→180 µs; frame 12→24 ~15.7→33.2 ms (debug). Threshold from frame curve; release-fast + streaming + 40-tile pending |
| Reliability       | Restart restores shape, sizes, focus, magnitude, and overflow set        |
| Safety            | No orphaned PTYs/workers on close, quit, or crash                       |
| Performance       | Grid stays within budget at the overflowing tile count                   |
| Orchestration     | Spawn → status → promote/demote → pause/resume/close workers end-to-end  |
| Usability         | A tile never becomes unusable: refuse or adapt, never spawn into nothing  |
| Packaging         | App + CLI smoke tests pass on macOS, Linux, Windows                       |
| Docs              | New user can launch a group and first worker without reading source       |
| Dogfooding        | Two weeks daily use with no layout loss, crash, or orphaned process       |

Nothing in Phase 2 (overflow) is a prerequisite for shipping Phases 0/1/3, and
no phase depends on a phase 4 feature. Phases are independently releasable.

### Next

- **Terminal workspace v1 polish** — better task routing, reopen-closed-tile
  behavior, and useful process status.
- **Cross-platform parity** — close macOS/Linux/Windows gaps in terminal input,
  packaging, shortcuts, and rendering.
- **Contributor experience** — make the build, test, release, and upstream-sync
  paths understandable from a clean checkout.

### Later, only with evidence

- Restore each tile's working directory and other session details, behind a
  setting if the complexity is justified.
- Explore dev containers or remote execution only if users need them and Rdg can
  own the distribution and compatibility story.
- Expand terminal groups beyond terminals only when the interaction remains
  simpler than a normal workspace tab.
- Persistent processes / headless worker containers surviving restart (the
  daemon subsystem PRD defers; `Worker` object is the seam).

## Explicit non-goals

These are not on the roadmap, and the following are explicitly **not** in v0.3.0:

- AI features or model-provider integrations.
- Account requirements or telemetry collection.
- Multiplayer collaboration, calls, channels, or peer-following.
- Rebuilding upstream Zed's remote-development infrastructure without a clear
  maintainer and release plan.
- **Headless / background worker containers** — v0.3 overflow is real attached
  PTYs. A worker that holds a log buffer but no visible PTY (and processes that
  survive restart) is the deferred daemon subsystem; the `Worker` abstraction is
  its seam, not its implementation.
- **Raising `max_tiles` as the overflow answer** — the cap is a performance and
  usability backstop. Overflow is a different surface, not a bigger number.
- **Agent-driven UI / workspace manipulation** — orchestration controls *worker*
  instructions and status, never the host UI.
- **Remote/SSH per-tile connections** and **non-terminal grid blocks** in
  v0.3.

## How to read status

| Status      | Meaning                                                 |
| ----------- | ------------------------------------------------------- |
| **Shipped** | Available on `main` or in a published release.          |
| **Now**     | Active focus; small, reviewable work is welcome.        |
| **Next**    | Validated direction, not the current sprint.            |
| **Later**   | Deliberately deferred until demand and ownership exist. |

For implementation detail, see the [terminal workspace PRD](./terminal-workspace-prd.md),
[collaboration removal PRD](./remove-collaboration-prd.md), and
[remote-development removal PRD](./remove-remote-development-prd.md).

## Contributing to the roadmap

Open an issue with the smallest user problem and a concrete success condition.
For larger work, include the affected platform, expected trade-offs, and how it
fits Rdg's local-first boundary. Roadmap decisions belong in public issues and
PRs so the reasoning remains reviewable.
