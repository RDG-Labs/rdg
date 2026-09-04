---
name: rdg-orchestration
description: Orchestrate visible external CLI workers in RDG Terminal Groups. Use when delegating tasks, spawning recursive workers, sending instructions, monitoring worker status, or managing an RDG swarm.
---

# RDG orchestration

RDG is the terminal control plane. Keep every worker as a normal visible external process. Do not invent agent-specific integrations, parse terminal screen output, or hide worker activity.

## Context

Managed workers inherit:

- `RDG_CONTROL_COMMAND`: the RDG control client path
- `RDG_GROUP_ID`: current Terminal Group
- `RDG_WORKER_ID`: current worker tile
- `RDG_PARENT_WORKER_ID`: parent worker tile, when present

Use the inherited control client when available:

```bash
RDG="${RDG_CONTROL_COMMAND:-rdg}"
```

If `RDG_GROUP_ID` or `RDG_WORKER_ID` is missing, the process was not launched by RDG. Ask the user to launch the root worker through **Terminal Group → + → Custom Command…** before attempting recursive orchestration.

## Spawn and delegate

Always capture the returned worker ID before sending work:

```bash
worker_json="$($RDG --control spawn "pi")"
worker_id="$(printf '%s' "$worker_json" | python3 -c 'import json,sys; print(json.load(sys.stdin)["Spawned"]["worker_id"])')"
$RDG --control send "$worker_id" "Inspect the authentication code. Report findings only; do not edit files."
```

Use any command, not just `pi`:

```bash
$RDG --control spawn "codex"
$RDG --control spawn "claude"
$RDG --control spawn "cargo test"
$RDG --control spawn "bash"
```

The spawned process inherits RDG context, so it may create descendants using the same protocol. Give every worker a specific task, scope, and expected report.

## Monitor workers

Get a structured snapshot:

```bash
$RDG --control list
```

Stream lifecycle events until interrupted:

```bash
$RDG --control watch
```

Events include `spawned`, `updated`, and `closed`. Keep terminal output visible for the user; use structured events for coordination rather than scraping output.

## Report status

Workers should report meaningful state transitions:

```bash
$RDG --control report "$RDG_WORKER_ID" working "Inspecting the auth flow"
$RDG --control report "$RDG_WORKER_ID" waiting "Need the orchestrator to choose between two fixes"
$RDG --control report "$RDG_WORKER_ID" completed "Fixed token refresh and passed tests"
$RDG --control report "$RDG_WORKER_ID" failed "Tests require unavailable service credentials"
```

Use concise summaries. Report completion only after verification.

## Coordinate workers

Send follow-up work to one worker:

```bash
$RDG --control send "$worker_id" "Implement the smallest safe fix and run the focused tests"
```

Send the same instruction to all visible workers or a comma-separated subset:

```bash
$RDG --control broadcast all "Stop editing and report your current state"
$RDG --control broadcast 123,456 "Run the focused test suite"
```

Prefer targeted sends for implementation tasks. Use broadcast for read-only coordination, pauses, or shared test commands.

## Safety rules

- Keep the root task and worker tasks explicit.
- Limit worker fan-out and depth; do not create recursive workers without a bounded plan.
- Never broadcast destructive commands unless the user explicitly requested it and every target is verified.
- Do not send a command to a worker that is already running an interactive prompt unless the worker is ready for input.
- Preserve user visibility: workers stay in real Terminal Group tiles.
- Treat worker summaries as claims until tests or artifacts verify them.
- Close or restart failed workers instead of silently spawning duplicates.
- Do not claim an external CLI completed work based only on a process title or terminal color.

## Recommended delegation pattern

1. Report the orchestrator as `working`.
2. Spawn the smallest useful number of workers.
3. Send each worker one bounded task.
4. Watch events and inspect worker reports.
5. Send follow-ups only after a worker reports `waiting`, `completed`, or an explicit checkpoint.
6. Verify the combined result in a worker or the orchestrator terminal.
7. Report a final summary with changed files and test commands.
