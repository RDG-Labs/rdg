# Rdg roadmap

Rdg is an open-source, terminal-first code editor. The roadmap is a product
commitment about direction, not a promise that every item will ship on a date.
Small fixes and user feedback can change the order.

**Last reviewed:** 2026-09-06 · [View the public project board](https://github.com/orgs/RDG-Labs/projects/1)

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

### Now

1. **Make terminal groups dependable**

   - Fix focus, move, close, resize, and tab-boundary edge cases.
   - Preserve layout safely across restart and workspace changes.
   - Keep the one-terminal-per-tile invariant intact under mouse, keyboard, task,
     and restore paths.
   - Turn the terminal workspace PRD into a maintained, testable specification.

2. **Raise release confidence**

   - Smoke-test the packaged app and `rdg` CLI on each supported platform.
   - Document install, upgrade, rollback, and troubleshooting paths.
   - Keep release artifacts and version behavior predictable for contributors.

3. **Build a healthy OSS feedback loop**
   - Keep issue forms Rdg-specific and easy to complete.
   - Triage reproducible bugs before adding new surface area.
   - Use roadmap issues for work that has an owner, a success condition, and a
     reason to exist.

### Next

- **Terminal workspace v1 polish** — better task routing, reopen-closed-tile
  behavior, useful process status, and performance under many active terminals.
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

## Explicit non-goals

These are not on the roadmap:

- AI features or model-provider integrations.
- Account requirements or telemetry collection.
- Multiplayer collaboration, calls, channels, or peer-following.
- Rebuilding upstream Zed's remote-development infrastructure without a clear
  maintainer and release plan.

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
