# PRD — Terminal Workspace ("Grid")

| | |
|---|---|
| **Status** | Draft for review |
| **Owner** | Irfan Saf (product), TBD (eng) |
| **Date** | 2026-08-31 |
| **Target** | Rdg (Zed fork) |
| **Supersedes** | Ad-hoc behavior established in #4 "Add center terminal split actions" |

---

## 1. Thesis

Developers run two tools side by side all day: an editor and a terminal multiplexer. Wave Terminal proved that a **directly manipulable tiled grid** beats tmux for exploratory work — you see every process at once, you rearrange by dragging, and you never memorize a prefix key. But Wave has no real editor. Zed has a world-class editor, and a terminal that is either a cramped dock strip or a center pane wearing a full tab strip.

**Rdg's bet: the tiled terminal grid becomes a first-class *tab* in the editor**, sitting next to file tabs, sharing the same project, file explorer, keymap, theme, and task system.

This is not a port of Wave. Wave's layout engine and Zed's `PaneGroup` are the *same data structure* — an n-ary flex tree of axes and leaves. The gap between what Rdg has today and the target is almost entirely **interaction design and quality guardrails**, not architecture. That is the cheapest kind of gap to close, and the most defensible: we can ship Wave's ergonomics *plus* the guardrails Wave gets wrong (§6.3).

### 1.1 Why now

Three things are already true in this repo:

1. `Member::{Axis, Pane}` in `crates/workspace/src/pane_group.rs:296` is an n-ary flex tree with per-axis `flexes: Vec<f32>`, cached `bounding_boxes`, and working `split` / `remove` / `swap` / `move_to_border`. It is structurally identical to Wave's `LayoutNode` (`frontend/layout/lib/layoutNode.ts`).
2. `TerminalPanel` at `crates/terminal_view/src/terminal_panel.rs:79` **already owns a nested `center: PaneGroup`** inside a dock panel. A container hosting its own pane tree is a solved, shipped pattern here.
3. `Pane` exposes `set_render_tab_bar`, `set_should_display_tab_bar`, `set_can_split`, `set_can_navigate` (`crates/workspace/src/pane.rs:831-876`). A pane can be reskinned into a bare tile with a slim header without forking `Pane`.

The terminal group now provides the tiled terminal workspace. The earlier center-terminal split actions were removed once terminal groups became the supported tiled-terminal surface.t.

---

## 2. Goals & non-goals

### 2.1 Goals (v1)

- **G1** A terminal grid is a single tab in the center tab strip, alongside editor tabs.
- **G2** One terminal per tile. No nested tab strips. Ever.
- **G3** Split, focus, resize, rearrange, magnify, and close are all reachable by both mouse and keyboard, with Wave-compatible muscle memory where Zed's keymap allows.
- **G4** The grid never lets you create an unusable tile (Wave's most visible defect — reproduced and documented in §14).
- **G5** Layout survives restart. Shape, proportions, focus, and magnify state are restored exactly.
- **G6** The implementation is confined enough that upstream Zed merges stay tractable (§12.2).

### 2.2 Explicit non-goals (v1)

| Non-goal | Rationale | Revisit |
|---|---|---|
| Shell processes surviving app restart | Requires a background daemon owning PTYs (Wave's `wavesrv`), IPC, scrollback storage, orphan reaping, and version-skew handling. A subsystem, not a feature. | Phase 4 |
| Restoring each tile's working directory | **Decided: layout only.** Machinery already exists (`terminal_view/src/persistence.rs` serializes cwd), so this is a deliberate simplification, not a limitation. | Phase 4, setting-gated |
| Per-tile remote/SSH connections | Pulls Zed's remoting layer onto the critical path. Data model reserves room (§5.4). | Phase 4 |
| Non-terminal blocks in the grid UI (web view, previews, AI) | Architecture supports it for free (§5.3); the UI is gated to terminals in v1 to keep scope honest. | Phase 4 |
| Replacing the bottom terminal dock | The dock is the right tool for a quick one-off command. Both coexist; §6.11 defines routing. | Never |
| Collaboration / follow-mode inside a grid | Zed's follower protocol tracks item + pane. Nested tile focus has no wire representation. | Post-v1 |
| A left file explorer | **Already shipped.** Zed's project panel is exactly the sidebar in the sketch. Zero work. | — |

---

## 3. Users and jobs

**Primary persona — the multi-service developer.** Runs 3–8 long-lived processes (API, worker, frontend dev server, tunnel, log tail, DB console) plus 1–2 scratch shells, while editing code in the same project.

Jobs to be done:

- **J1** "Show me every running service at once, without switching." → grid, always visible.
- **J2** "Give me a scratch shell right here, now." → one keystroke split, no dialog.
- **J3** "This one is misbehaving — let me look closely without losing my place." → magnify.
- **J4** "Rearrange these so related things sit together." → drag by header.
- **J5** "Come back tomorrow to the same wall of terminals." → layout restore.
- **J6** "Edit the file this stack trace points at." → the grid is a tab, so the editor is one tab away with the same project panel.

**Anti-persona.** The user who wants one maximized terminal. They can still use `workspace::NewTerminal`. The grid must not tax them: it is opt-in, never the default.

---

## 4. Vocabulary

Precise words, used consistently in code, UI, settings, and docs.

| Term | Definition |
|---|---|
| **Terminal Group** | A workspace `Item` that appears as one tab and contains a tile tree. The unit the sketch labels "terminal group tab". |
| **Tile** | One leaf of the tree. Contains exactly one terminal plus a header. Never contains tabs. |
| **Tile tree** | The recursive structure: an *axis* (row or column) of *members*; a member is a tile or a nested axis. |
| **Gutter** | The draggable gap between two sibling members. |
| **Focus** | Exactly one tile per group is focused; its terminal receives keystrokes. Rendered as a 1px accent ring. |
| **Magnify** | A focused tile temporarily floats above the dimmed grid at ~92% of the group's bounds. Non-destructive; the tree is unchanged. |
| **Active tile** | The focused tile of the *active* group. There is at most one in the window. |

Rejected words: "pane" (means something else in Zed), "block" (Wave's word; ours are terminals in v1), "window" (OS-level).

---

## 5. Architecture

### 5.1 Decision: Terminal Group as an Item

Three options were considered.

| | **A. Group-as-Item** ✅ | B. Grid mode on center pane group | C. Replace the center with a Wave-style tree |
|---|---|---|---|
| Matches the sketch | Yes — one tab = one grid | No — one tab per tile | Yes |
| Editor tabs alongside | Yes, unchanged | Yes | Requires reinventing editor tabs |
| Blast radius on Zed core | Small; new crate + extension points | Medium; rewrites center rendering | Very large |
| Upstream merge cost | Low | High | Prohibitive for a fork |
| Hard problems | Nested focus/action routing, drag across the nesting boundary, zoom vs magnify | Every tile grows a tab strip | Everything |

**Chosen: A.** It mirrors `TerminalPanel`'s existing nested `PaneGroup`, it is a literal match to the sketch, and it keeps the diff off `editor` entirely.

### 5.2 Decision: tiles are `Pane`s, presented as single-terminal cards

A tile is a `Pane` configured with `set_should_display_tab_bar(|_,_| false)` and a custom slim header, holding exactly one `TerminalView`.

We get, for free and already tested upstream: split, remove-with-axis-collapse, swap, flex resize, bounding-box hit testing, item lifecycle, focus handling, and serialization. We pay one price: we must **enforce** the one-terminal invariant on every path that could add a second item to a tile (§6.12). That enforcement is a bounded list of rules; reimplementing a parallel tree type is not.

> **Invariant TG-1.** A tile contains exactly one item, and that item is a `TerminalView`. Any operation that would violate this instead creates a new tile. This is enforced in one place — the tile's item-added guard — and covered by tests.

### 5.3 Why this scales beyond terminals for free

Because a tile is a `Pane`, it can host **any** Zed `Item` — editors, markdown previews, images, diagnostics — with no change to the tree. The v1 restriction to terminals is a *UI gate*, not a structural one. Phase 4 opens the gate; it does not rewrite the engine. This is the single most important scalability property of the design and the reason option A beats a bespoke tree.

### 5.4 Data model

```rust
// Serialized with the workspace, versioned for migration.
struct SerializedTerminalGroup {
    version: u32,               // schema version; migration path per §7 E17
    root: SerializedTileTree,
    focused_tile: Option<TileId>,
    magnified_tile: Option<TileId>,
    title: Option<SharedString>, // user-renamed group tab
}

enum SerializedTileTree {
    Axis { axis: Axis, flexes: Vec<f32>, children: Vec<SerializedTileTree> },
    Tile(SerializedTile),
}

struct SerializedTile {
    id: TileId,
    // v1: nothing else is restored (layout-only decision, §2.2).
    // Reserved, written as None, ignored on read until the phase lands:
    working_directory: Option<PathBuf>,  // Phase 4
    connection: Option<ConnectionId>,    // Phase 4
    title_override: Option<SharedString>,
}
```

Reserved fields are written as `None` from v1 so that a Phase 4 build reading a v1 layout needs no migration in the common case.

### 5.5 Nesting contract

Three levels of tree now exist. Ambiguity here is the top implementation risk (§12.1), so the routing rule is stated once and normatively:

```
Workspace pane tree  →  Pane  →  tab strip  →  TerminalGroup (Item)  →  tile tree  →  Tile (Pane) → TerminalView
```

> **Rule TG-2 (action routing).** An action dispatched while a tile has focus resolves against the **innermost** context that handles it. `TerminalTile` context handles split/close/focus/resize/magnify and consumes them. Actions the tile does not handle (`workspace::*`, `pane::CloseActiveItem` targeting the group tab, panel toggles) propagate to the outer pane and workspace unchanged.

> **Rule TG-3 (split disambiguation).** `SplitRight` with a tile focused splits **the tile**, not the workspace pane. To split the workspace pane while inside a grid, users use `cmd-k` chord bindings, which the tile context deliberately does not claim — with one documented exception: `cmd-k` inside a focused terminal is `terminal::Clear` (existing Zed behavior), so chorded workspace splits are unavailable while typing in a terminal. This is pre-existing Zed behavior, not a regression, and is called out in docs.

---

## 6. Behavior specification

### 6.1 Creating a group

| Entry point | Behavior |
|---|---|
| Command palette → "Terminal Group: New" | Opens a new group tab in the active workspace pane with one tile, focused, shell in the project root. |
| Tab-bar `+` overflow menu | New entry "New Terminal Group". The obsolete center-terminal split entries are no longer shown. |
| `workspace::NewTerminalGroup` action | Same as palette; bindable. |
| Promote from dock | Dock terminal context menu → "Move to Grid": creates a group containing that terminal, moving it (not copying). Dock closes if it was the last terminal there. |

A new group's tab title is `Terminal` (or `Terminal 2`, `Terminal 3`… disambiguated per workspace). The tab is renameable via double-click, persisted in `SerializedTerminalGroup::title`.

Cold start of the first tile must reach a visible prompt in **< 250 ms** on a warm shell.

### 6.2 Creating tiles

**Explicit split.** `SplitRight` / `SplitDown` / `SplitLeft` / `SplitUp` on the focused tile. The new tile is inserted as a sibling on the requested side, spawns a shell in the project root, and **takes focus** (matching Wave, verified live).

**Sizing on insert.** This is where we deviate from Wave deliberately.

- Wave resets all siblings on the axis to equal size. So does Zed today (`PaneAxis::insert_pane` sets `flexes = vec![1.; n]`). Verified live: splitting a 3-row column re-equalized all three.
- **Rdg behavior:** the split **takes space only from the tile being split**. The splitting tile's flex is halved and the new tile receives the other half; every other sibling keeps its exact flex. Rationale: a carefully sized grid must not be destroyed by adding one scratch shell. This is a strictly better behavior and cheap to implement.
- Double-click any gutter, or `Terminal Group: Equalize`, restores equal sizing on that axis. `Terminal Group: Equalize All` normalizes the whole tree.

**Implicit new tile** (`Terminal Group: New Tile`, no direction). Auto-placement walks the tree for the first axis with fewer than **5** children (Wave's `DEFAULT_MAX_CHILDREN`, adopted as-is — it produces good-looking grids) and appends there; if none qualifies, it splits the largest tile along its longer edge. Deterministic, so the same sequence always yields the same grid.

### 6.3 The split guard — our headline quality differentiator

**The defect, measured live in Wave (§14):** six consecutive `Cmd+D` presses produced seven tiles ~48 px wide in one row. Headers degraded to two icons with the close button gone. Terminals reflowed to roughly 8 columns, wrapping every prompt into unreadable ribbons. Wave's `MinNodeSizePx = 40` guards only *resize*, and `DEFAULT_MAX_CHILDREN = 5` guards only *auto-placement* — neither guards an explicit split. There is no undo.

**Rdg requirement.** A split that cannot produce a usable tile must not happen.

Usability is defined in **character cells**, not pixels, because font size varies:

```
min_tile_columns = 30   // enough for a prompt plus a short path
min_tile_rows    = 8    // prompt + a few lines of output
```

Resolution order when `SplitRight` is requested but the resulting width would be under `min_tile_columns`:

1. **Adapt** — if splitting on the *other* axis satisfies both minimums, do that instead and show a one-line status hint: "Split below — not enough width." (default; `split_guard: "adapt"`)
2. **Refuse** — if neither axis fits, do nothing except a transient toast: "Not enough room for another terminal. Magnify, close a tile, or open a new group." **No shell is spawned.** Spawning a process the user cannot see or use is the real harm.
3. `split_guard: "off"` restores Wave's behavior for users who want it.

The same guard applies to drag-drop insertion (§6.7) and to auto-placement (§6.2).

**This requirement is non-negotiable for v1.** It is the clearest, most demonstrable way Rdg's grid is better than the tool it learns from.

### 6.4 Focus

- **Click** anywhere in a tile focuses it, including its header. Click-through: the click that focuses a terminal also positions the cursor/selection as Zed's terminal already does.
- **Directional navigation** — `ctrl-shift-{arrows}` and `ctrl-shift-{h,j,k,l}` (Wave parity; both free in Zed's Terminal context). Movement uses the geometric neighbor by bounding box, not tree order, so navigation feels spatial. At an edge, focus does **not** wrap and does **not** leave the group; it is a no-op. (Leaving the grid is `ctrl-shift-e` to the project panel, `cmd-1` etc. — existing Zed bindings, unclaimed by the tile context.)
- **Cycle** — `ctrl-tab` inside a group cycles tiles in reading order (row-major, depth-first). Wraps.
- **Focus ring** — 1px accent border on the focused tile, plus a subtly brighter header. The unfocused tiles' headers dim to 60% opacity. In Wave the ring is the only signal and it reads well; we add the header treatment because our tiles are larger.
- **Focus follows mouse** honors the existing workspace setting. When on, hovering a tile focuses it after a 150 ms dwell (dwell prevents focus thrash while crossing the grid to reach a far tile).
- **On close of the focused tile**, focus moves to the nearest sibling in the same axis, preferring the one *before* it; if the axis collapses, to the tile occupying the vacated space.

### 6.5 Resize

- **Gutter drag.** Grab anywhere in the 6 px gap plus a 3 px overshoot on each side (12 px effective target — Fitts's law; Wave's 3 px gutters are fiddly). Cursor changes to the axis resize cursor on hover.
- **Live preview** with the terminal reflowing continuously, **but PTY resizes are coalesced** (§8.3).
- **Minimums.** A drag cannot take a sibling below `min_tile_columns` × `min_tile_rows`. The gutter stops hard at that boundary rather than continuing invisibly — the pointer may detach from the gutter, which is correct and matches every good tiling UI.
- **Keyboard resize.** `ctrl-alt-shift-{arrows}` grows/shrinks the focused tile by 5% of its axis per press.
- **Double-click a gutter** equalizes that axis.
- **Window resize** scales all tiles proportionally. If proportional scaling would take tiles below the minimum, the grid **does not** reshape itself; it allows tiles below minimum and marks them (§7 E1). Reshaping a user's layout because they resized a window is unacceptable; showing an honest degraded state is not.

### 6.6 Magnify

Verified live in Wave: the magnified block floats at roughly 90% of the container over a blurred, dimmed grid; the layout tree is untouched; `Cmd+M` toggles.

Rdg behavior:

- Trigger: `shift-escape` (Zed's existing `workspace::ToggleZoom` binding, reused in `TerminalTile` context — zero new conflicts and the same mental model), the header's magnify button, or double-click the header.
- Geometry: the tile animates to `magnify_size` (default **0.92**) of the group's bounds, centered, 150 ms ease-out.
- Backdrop: the grid behind is **dimmed to 40% opacity, not blurred.** Wave blurs (`window:magnifiedblockblurprimarypx`); a full-viewport backdrop blur is a per-frame GPU cost on a surface that is already compositing N terminals. Dimming is visually sufficient and free.
- Escape hatches: `shift-escape` again, `Escape` **only if** the terminal is not in a mode that consumes it — since Zed's terminal binds bare `escape` to `SendKeystroke`, bare Escape does **not** un-magnify. Clicking the dimmed backdrop does.
- The grid stays visible behind the magnified tile. This is the whole point: you keep your sense of place, unlike Zed's current zoom which blanks everything.
- Interactions: splitting while magnified un-magnifies first, then splits. Closing the magnified tile un-magnifies and focuses per §6.4. Switching to another tab preserves magnify state for when you return. Magnify state is persisted.
- Only one tile per group may be magnified. Workspace-level dock zoom and tile magnify are mutually exclusive: magnifying dismisses a zoomed dock (Zed's existing `dismiss_zoomed_items_to_reveal` already does this).

### 6.7 Rearranging by drag

Verified live in Wave; three distinct drop semantics were reproduced and are adopted with modifications.

**Drag handle:** the tile header. Dragging the terminal body selects text, as it must.

**During drag:**
- The source tile ghosts to 40% opacity in place (Wave does this; it reads well).
- A small card preview follows the cursor.
- The drop target renders a filled accent region — *not* a thin line — showing the exact shape the tile will occupy. Wave's filled preview is meaningfully clearer than a line and costs nothing.

**Drop zones**, evaluated against the tile under the cursor:

| Zone | Region | Result |
|---|---|---|
| **Center** | Inner 50% × 50% | **Swap** the two tiles. Sizes stay with the positions, not the tiles. |
| **Edge** | Outer band, 25% of the tile's width/height on each side | Insert as a sibling on that side *within the target's axis*. |
| **Outer edge** | Within 24 px of the group's outer bounds | Insert as a new child of the **root** axis — a full-height column or full-width row. Verified live; this is how you promote a tile out of a nested column. |

Additional rules:

- **The split guard applies.** A drop that would create an unusable tile is rejected: the drop region renders in a muted "not allowed" treatment and the drop is a no-op. Wave permits it.
- **Escape cancels** an in-flight drag, restoring the source. (Wave has no cancel; this is a gap.)
- **Dropping on the source tile's own center** is a no-op, not a self-swap.
- **Dropping on a group tab in the tab strip** moves the tile into that group, auto-placed per §6.2.
- **Dragging a tile out onto the workspace tab strip** converts it to a normal center terminal tab and removes it from the grid. Reverse of "promote".
- **Dragging an editor tab into a grid** is rejected in v1 with an explicit not-allowed cursor. `set_can_split` already gives us the hook, and TG-1 requires the rejection. The gate lifts in Phase 4.
- **Axis collapse on removal** is automatic and already correct in `PaneAxis::remove`: verified live in Wave and in Zed's existing code — when a column drops to one child, the column dissolves and the child takes its place.
- **Sizes on removal:** the departing tile's flex is redistributed proportionally among its remaining siblings, so relative proportions are preserved.

### 6.8 Closing

| Action | Behavior |
|---|---|
| Close tile (header ×, `cmd-w` in tile context) | Closes the tile. If a foreground process other than the shell is running and `terminal.confirm_close` is on, prompt with the process name. |
| Close last tile in a group | Closes the group tab. Same confirmation rules. |
| Close group tab (`cmd-shift-w`, tab ×) | If any tile has a running process, one consolidated confirmation lists them — "3 terminals are running: npm, cargo, ssh" — with Cancel / Close All. Never N sequential dialogs. |
| Close window / quit with running processes | Existing Zed behavior, unchanged. |

**`cmd-w` in tile context closes the tile, not the group tab.** This matches Wave and is the behavior a grid user expects. `cmd-shift-w` closes the group. Documented prominently because it inverts Zed's default meaning — see §6.13.

Closed tiles enter the workspace's reopen-closed-item history, so `cmd-shift-t` restores an accidentally closed tile into its previous position when the shape still permits it, otherwise auto-placed.

### 6.9 Tile header

A 24 px bar, always visible (not hover-revealed — the sketch shows persistent cards, and hover-reveal headers make drag handles undiscoverable).

Layout, left to right: **status dot · title · spacer · magnify · close**, with an overflow menu appearing only when the header is too narrow for both buttons.

- **Title** resolves in priority order: user override → foreground process name → shell title (OSC 0/2) → shell name. Prefixed with the tile's cwd basename when it differs from the project root: `api ▸ npm run dev`.
- **Status dot:** running (accent), idle at prompt (dim), exited 0 (dim), exited non-zero (error color, and the tile border tints on the failing edge for 3 s so a failure across the grid is visible peripherally).
- **Truncation:** middle-ellipsis, keeping the process name and dropping host/path first. Verified live: Wave truncates to `irfansaf@Saf-M...`, which hides the *only* useful part. We drop the host first, then the path, then ellipsize the process name last.
- **Degradation under narrow widths:** below ~180 px the close button collapses into the overflow menu; below ~120 px only the status dot and a truncated title remain. Because of §6.3 this state is reachable only by window shrink, never by splitting.

### 6.10 Persistence

**Saved:** tree shape, per-axis flexes, tile identity, focused tile, magnified tile, group title, and each group's position in the workspace tab strip.

**Not saved (v1, per §2.2):** working directories, scrollback, shell state, environment.

**When:** debounced 300 ms after the last mutation; forced on tab deactivate, window blur, and clean shutdown. Serialization runs off the main thread; it must never appear in a frame trace.

**Restore:** the group tab reappears with the exact grid shape. Each tile shows its header with a dim status dot and an empty body. Shells are **spawned lazily** (§8.4) — this makes a 12-tile grid restore feel instant instead of forking twelve processes into a cold app.

**Restore failures:** if the project root no longer exists, the group restores with tiles in an error state showing the missing path and a "Choose folder" action, rather than silently spawning shells in `$HOME`.

### 6.11 Relationship to the dock terminal and to tasks

Both terminal surfaces exist. Routing is explicit so nothing is surprising:

| Trigger | Destination |
|---|---|
| `terminal_group::New` (`ctrl-` `) | Opens a new terminal-group tab. |
| `workspace::NewTerminal` | Opens a standard terminal using the existing terminal workflow. |
| Obsolete center-terminal actions | Removed; tile creation belongs to the terminal group. |
| Task run, `terminal.dock` default | Dock, unchanged |
| Task run **while a tile is focused** | New tile in the current group, auto-placed. Rerunning the same task reuses that tile (Zed's existing reuse semantics) — which does **not** violate TG-1, since it replaces the terminal rather than stacking one. |
| "Run in grid" from the task picker | Explicit new tile |

### 6.12 The one-terminal invariant — enforcement points

Every path that could put a second item into a tile, and its resolution:

| Path | Resolution |
|---|---|
| Drop an editor tab on a tile | Rejected (v1 gate) |
| Drop a terminal tab on a tile's **center** | Swap, not stack |
| Task spawns into the active pane | Redirected to a new tile |
| `reopen closed item` targeting a tile | Auto-placed as a new tile |
| Restore of a legacy layout with a multi-item pane | Split into one tile per item at load, preserving order left-to-right |
| Any programmatic `add_item` on a tile pane | Guard returns the item to the group, which auto-places it |

One guard function, six call sites, six tests.

### 6.13 Keymap

Verified against `assets/keymaps/default-macos.json`. All bindings live in a new `TerminalTile` context, which is more specific than `Terminal` and therefore wins resolution.

| Action | Binding | Conflict analysis |
|---|---|---|
| Split right | `cmd-d` | **Already `pane::SplitRight` in Zed's Terminal context** (line 1371). We rebind the *target*, not the key. Wave parity with zero muscle-memory cost. |
| Split down | `cmd-shift-d` | Overrides `debug_panel::ToggleFocus` (Workspace context, line 761) while a tile is focused. Accepted: Wave parity is worth more here, and the debug panel remains available via palette and via `cmd-shift-d` everywhere else. Non-conflicting alternative `ctrl-alt-down` is also bound (already `pane::SplitDown` in Terminal context). |
| Split left / up | `ctrl-alt-left` / `ctrl-alt-up` | Already bound to pane splits in Terminal context; retargeted. |
| Magnify toggle | `shift-escape` | Zed's `workspace::ToggleZoom` (line 30). Same concept, retargeted to the tile. **Zero new conflicts.** Deliberately *not* `cmd-m` — that is `zed::Minimize` (line 40) and stealing it from macOS is hostile. |
| Focus neighbor | `ctrl-shift-{arrows}`, `ctrl-shift-{h,j,k,l}` | Free in Terminal context (only `ctrl-shift-space` is taken). Wave parity. |
| Cycle tiles | `ctrl-tab` | Scoped to the group; falls through to `tab_switcher` outside. |
| Swap with neighbor | `ctrl-alt-shift-{arrows}` | Free. Not `cmd-k shift-{arrow}` (Zed's `SwapPane`), because `cmd-k` is `terminal::Clear` inside a terminal. |
| Resize | `ctrl-alt-shift-{arrows}` with modifier held, or `Terminal Group: Resize` mode | See §6.5 |
| Close tile | `cmd-w` | Overrides `pane::CloseActiveItem`. Inverts Zed's default meaning — the highest-friction decision in this table. Mitigations: `cmd-shift-w` closes the group; the change is documented in the group's empty state and release notes; a setting `terminal_workspace.cmd_w_closes: "tile" \| "group"` lets Zed natives keep the old meaning. |
| Equalize axis | double-click gutter | — |

`use_key_equivalents: true` throughout, matching the rest of the keymap.

### 6.14 Empty and error states

- **Empty group** (only reachable transiently): centered prompt with the three primary actions and their keys.
- **Tile awaiting lazy spawn:** dim header, empty body, no spinner — a spinner for a 40 ms operation is noise.
- **Shell failed to spawn:** the tile shows the error and a Retry action, matching `FailedToSpawnTerminal` in the existing panel.
- **Tile below minimum size** (window shrink only): body replaced by a compact "Too small" chip with a magnify affordance. The terminal keeps running and keeps its PTY at the last valid size — it is *not* resized to an absurd geometry.

### 6.15 Settings

```jsonc
"terminal_workspace": {
  "gap": 6,                      // px between tiles
  "corner_radius": 6,            // matches the sketch's rounded cards
  "min_tile_columns": 30,        // §6.3 usability floor, in cells
  "min_tile_rows": 8,
  "split_guard": "adapt",        // "adapt" | "refuse" | "off"
  "magnify_size": 0.92,          // fraction of group bounds
  "magnify_backdrop": "dim",     // "dim" | "none"
  "header": "always",            // "always" | "hover"
  "focus_follows_mouse_dwell_ms": 150,
  "deferred_spawn": true,        // §8.4
  "max_tiles_soft": 16,          // warn above this
  "max_tiles_hard": 32,          // refuse above this
  "cmd_w_closes": "tile"         // "tile" | "group"
}
```

All settings are live-reloadable; changing `gap` or `corner_radius` must not remount terminals (which would clear scrollback).

---

## 7. Edge cases

| # | Case | Required behavior |
|---|---|---|
| E1 | Window shrunk until tiles fall below minimum | Layout is preserved, not reshaped. Sub-minimum tiles show the "Too small" chip (§6.14) and hold their last valid PTY size. Restoring window size restores the terminals exactly. |
| E2 | Layout restored into a window smaller than when saved | Same as E1. Never silently discard tiles. |
| E3 | A tile streams output at high rate (`yes`, verbose build) | Terminal repaint is throttled per §8.2. Other tiles must not drop frames. |
| E4 | Many tiles stream simultaneously | Aggregate repaint budget enforced; unfocused tiles degrade to a lower repaint rate before the focused tile does. |
| E5 | Gutter dragged fast across the whole group | PTY resize coalesced (§8.3); at most one `SIGWINCH` per tile per 50 ms plus one authoritative resize on release. |
| E6 | Shell exits (`exit`, crash) | Tile stays with an exited status dot and exit code. Honors the existing `terminal.working_directory`/close-on-exit settings. Never silently removes the tile — that would reshape the grid under the user. |
| E7 | Split requested with no room | §6.3. **No process is spawned.** |
| E8 | Drag started, then the source tile's process exits mid-drag | Drag continues; the tile moves in its exited state. |
| E9 | Drag started, then the target tile is closed mid-drag (e.g. by a script) | Drop target recomputed on the next frame; if it no longer exists, the drop falls back to the nearest valid region or cancels. Never panics. |
| E10 | Two groups in two split workspace panes | Fully independent. Exactly one tile in the window is *active*; the other group renders its focused tile with an inactive (dimmed) ring, matching Zed's inactive-pane treatment. |
| E11 | A group tab dragged to another workspace pane | Moves whole, with its tree, focus, and magnify state. |
| E12 | Group tab dragged to another **window** | Same, via existing item-transfer machinery. Terminals move; they are not respawned. |
| E13 | Magnified tile while the group tab is inactive | Magnify state held; not rendered. Restored on reactivation. |
| E14 | A dock is zoomed, then a tile is magnified | Dock zoom dismissed first (existing `dismiss_zoomed_items_to_reveal`). |
| E15 | Tree depth grows pathologically (drag loops) | Depth capped at 8; deeper insertions flatten into the nearest ancestor axis of matching orientation. Wave's `addChildAt` already flattens same-orientation nesting — we adopt the same normalization. |
| E16 | Tile count exceeds soft/hard caps | Soft: toast warning once per session. Hard: split/insert refused with the §6.3 toast. |
| E17 | Layout schema changes between versions | `version` field. Unknown future version → group restores as a single tile with a non-blocking notice, never a crash and never silent data loss. |
| E18 | Remote (SSH) project | Tiles inherit the project's connection, as center terminals do today. No per-tile connection UI in v1. |
| E19 | Collaboration: a follower follows a host who focuses a tile | Follower follows to the *group tab*; tile-level focus is not transmitted (documented limitation, §2.2). |
| E20 | Screen reader / keyboard-only user | Every tile reachable by keyboard; the header exposes an accessible label `"Terminal N of M: <title>, <status>"`; drag has a keyboard equivalent (swap bindings, §6.13); magnify announces state change. |
| E21 | `terminal.font_size` changed while a grid is open | All tiles re-measure. Tiles that fall below the cell minimum enter the E1 state rather than reshaping the grid. |
| E22 | Restore when the project root was deleted | §6.10. Error tiles with a "Choose folder" action; no shells spawned into `$HOME`. |
| E23 | Rapid split spam (key held down) | Splits are serialized and each re-evaluates the guard, so the guard cannot be outrun. Auto-repeat is rate-limited to 10/s. |
| E24 | Undo of a destructive layout change | `cmd-shift-t` restores closed tiles (§6.8). Layout *shape* changes (drag, resize) are **not** undoable in v1 — an explicit, documented gap; Wave has none either. Tracked as a Phase 3 candidate. |

---

## 8. Performance requirements

The grid renders N live terminals simultaneously. Performance is a **feature**, and these are acceptance thresholds, not aspirations.

### 8.1 Budgets

| Scenario | Threshold |
|---|---|
| 12 idle tiles, cursor blinking | Zero dropped frames at 60 Hz; p95 frame ≤ 8 ms |
| 4 of 12 tiles streaming ~1000 lines/s | p95 frame ≤ 16 ms; focused tile never drops below 30 fps |
| Split → new tile painted | ≤ 50 ms |
| Gutter drag | ≤ 8 ms/frame; no PTY resize on the render path |
| Magnify animation | 150 ms, no dropped frames |
| Group restore, 12 tiles | Grid painted ≤ 150 ms (shells deferred) |
| Layout serialization | ≤ 2 ms, off the main thread |
| Memory | ≤ 40 MB per idle tile at default 10k-line scrollback |

### 8.2 Repaint throttling

Zed's terminal already batches PTY reads. The grid adds a **tier**: the focused tile repaints at display rate; unfocused tiles repaint at a capped rate (default 20 Hz) and coalesce their pending output. A tile whose bounds are fully occluded (behind a magnified tile) does not repaint at all — it accumulates and paints once on reveal. This is the single highest-leverage optimization and it is what makes 16 tiles viable.

### 8.3 PTY resize coalescing

Naive implementations emit `SIGWINCH` on every mouse-move during a gutter drag. Full-screen TUIs (vim, htop, less) redraw completely on each one, so a 500 ms drag can trigger 30 full redraws across every affected tile — the dominant source of jank in tiled terminals.

Requirement: during an interactive resize, a tile receives at most **one resize per 50 ms**, and exactly **one authoritative resize on release**. Intermediate frames scale the already-rendered grid visually without touching the PTY.

### 8.4 Deferred shell spawn

On restore, a tile does not fork a shell until the first of: the tile gains focus; the group tab has been active for 500 ms; or the user interacts with the tile. Spawns are limited to 4 concurrent. A 12-tile grid therefore costs one process at restore instead of twelve, and the app stays responsive. Controlled by `deferred_spawn`.

### 8.5 Rendering discipline

- Reuse `PaneAxis::bounding_boxes` (already cached) for hit testing; never walk the tree during a mouse-move.
- Recompute layout for the mutated subtree only, not the whole tree.
- Magnify backdrop is a dim overlay, never a blur (§6.6).
- Drop-target previews are a single quad, not per-tile overlays.
- The 6 px gaps are drawn by the container, not as per-tile margins, so tile bounds stay pixel-aligned and text never lands on a half pixel.

---

## 9. Success criteria

Telemetry and account connectivity were deliberately removed from this fork (commit 7fd594a). **We therefore do not have, and will not add, product analytics.** Success is measured by acceptance tests and a dogfooding checklist — which is the honest instrument for a tool with one-to-few users anyway.

**Ship gate for v1** — all must pass:

1. Every behavior in §6 has an automated test, following the pattern of the existing `test_new_center_terminal_split_creates_multiple_panes`.
2. Every edge case in §7 is either tested or has a written, linked justification for why it is not.
3. Every threshold in §8.1 is measured on a 12-tile grid and recorded in the PR.
4. **The Wave failure is not reproducible in Rdg:** ten consecutive splits from a single tile in a 1400×900 window produce a usable grid or a refusal, never an unusable tile. This is the single acceptance test that defines the feature.
5. Two weeks of daily dogfooding by the author with no layout loss, no crash, and no orphaned processes.
6. `./script/clippy` clean; no new `unwrap()` on any path reachable from user input.

---

## 10. Phasing

| Phase | Scope | Exit criteria |
|---|---|---|
| **0 — done** | Terminal-group foundation | Shipped. The obsolete center-terminal split actions were removed after the terminal group became available. | |
| **1 — Foundation** | `TerminalGroup` item; tile tree; split/close/focus/resize; **split guard**; slim headers; layout persistence; keymap | Ship gate 1, 2, 4, 6. A grid is usable daily without drag or magnify. |
| **2 — Direct manipulation** | Drag to rearrange with all three drop zones; magnify; swap bindings; equalize | Ship gate 3. Feature-complete vs. the sketch. |
| **3 — Polish & performance** | Repaint tiering; PTY coalescing; deferred spawn; status dots; title resolution; accessibility; docs | Ship gate 5. All §8 thresholds met. |
| **4 — Expansion (deferred)** | cwd restore; heterogeneous blocks (the gate in §5.3); per-tile connections; persistent processes | Separate PRDs. Each is independently valuable and independently shippable because of §5.3. |

Phases 1–3 are sequential; nothing in 4 is a prerequisite for anything in 1–3.

---

## 11. Open questions

| # | Question | Owner | Blocks |
|---|---|---|---|
| Q1 | Is `cmd-w` closing a *tile* acceptable, or should the default be `cmd_w_closes: "group"` with tile-close on `cmd-shift-w`? | Irfan | Phase 1 keymap |
| Q2 | Should a group tab show a live badge (e.g. "3 running / 1 failed") in the tab strip? Valuable when the group is in a background tab; costs tab-strip real estate. | Irfan | Phase 3 |
| Q3 | Should layout *shape* changes be undoable (E24)? Cheap if the tree is snapshotted per mutation; adds a memory tail. | Eng | Phase 3 |
| Q4 | Named layout presets ("services", "debug") that can be applied to a group — worth it, or does it duplicate saved workspaces? | Irfan | Phase 4 |

---

## 12. Risks

### 12.1 Nested focus and action routing — **high**

Three tree levels with overlapping action vocabularies is where this design can rot. Mitigation: rules TG-2 and TG-3 are normative and testable; the `TerminalTile` context claims an explicit, closed list of actions and forwards everything else untouched; a test asserts that every action in that list resolves to the tile and that a representative set of workspace actions still reaches the workspace.

### 12.2 Fork divergence from upstream Zed — **high**

Rdg tracks upstream. Every line changed in `workspace.rs` or `pane.rs` is a future merge conflict, and those two files are 19k and 9.7k lines of actively developed code.

Mitigation, in priority order:
1. New crate `terminal_group`, with `[lib] path = "src/terminal_group.rs"` per the repo's crate conventions. All new logic lives there.
2. Use existing extension points (`set_render_tab_bar`, `set_should_display_tab_bar`, `set_can_split`, `set_can_navigate`) rather than adding new ones.
3. Where a hook genuinely does not exist, add a **narrow, additive** one — a new method, never a changed signature — and keep each such change to a single hunk so conflicts resolve trivially.
4. Budget: **≤ 150 changed lines total across `workspace.rs` and `pane.rs`.** If a design needs more, redesign. Track the number in the PR description.

### 12.3 Keybinding conflicts — medium

Mitigated by §6.13's verified analysis and the `cmd_w_closes` setting. Residual risk is muscle-memory confusion for `cmd-shift-d`; documented in release notes.

### 12.4 PTY resize storms — medium

Mitigated by §8.3. Verify explicitly with `htop` running in four tiles during a sustained gutter drag.

### 12.5 Scope creep toward "Wave in Rust" — medium

Wave has 17 block types, an AI assistant, a daemon, and a remote-connection system. Every one is tempting. Mitigation: §2.2 is binding, and §5.3 means saying no today costs nothing tomorrow.

---

## 13. What we deliberately do differently from Wave

| Area | Wave | Rdg | Why |
|---|---|---|---|
| Split into no space | Allowed; produces ~48 px unusable tiles | Adapt axis, else refuse; no process spawned | §6.3 — the headline quality bar |
| Sizing on split | Re-equalizes all siblings | Takes space only from the split tile | Preserves a deliberately tuned grid |
| Magnify backdrop | Blur (per-frame GPU cost) | Dim | §8.5 |
| Drag cancel | None | `Escape` | Basic direct-manipulation hygiene |
| Header truncation | Drops the useful part (`irfansaf@Saf-M...`) | Drops host, then path, keeps process name | §6.9 |
| Restore | Reattaches live processes (daemon) | Layout only, lazy spawn | Deliberate v1 simplification; §2.2 |
| Editor | None | Full Zed editor in the same tab strip | The reason this fork exists |

---

## 14. Appendix — Wave teardown, measured live (2026-08-31)

Conducted by driving the running Wave app directly. Findings that shaped this document:

1. **Drop semantics.** Hovering a tile's center highlights the *entire* target tile → swap. Hovering an edge highlights a band *within* the target's axis → insert as sibling. Hovering near the group's outer bounds highlights a *full-height* band → insert at the root axis. Three distinct, learnable behaviors. Adopted (§6.7).
2. **Axis collapse.** Moving a tile out of a 2-child column dissolved the column and promoted the remaining child to full height. Matches Zed's existing `PaneAxis::remove`. No work needed.
3. **Size reset on insert.** Inserting into a 2-row column produced three exactly equal rows, discarding prior proportions. Rejected (§6.2).
4. **Split failure mode.** Six consecutive `Cmd+D` presses produced seven ~48 px tiles in one row; headers lost their close buttons, terminals reflowed to ~8 columns, and there was no undo. Root cause in source: `MinNodeSizePx = 40` guards only resize (`layoutModel.ts:73`), and `DEFAULT_MAX_CHILDREN = 5` guards only auto-placement (`layoutTree.ts:35`). Explicit splits are unbounded. **This is the defect §6.3 exists to prevent.**
5. **Magnify.** Floats at ~90% over a blurred, dimmed grid; tree untouched; `Cmd+M` toggles; size configurable via `window:magnifiedblocksize`. Adopted with dim instead of blur (§6.6).
6. **Focus on split.** The newly created tile takes focus. Adopted (§6.2).
7. **Keymap.** `Cmd+D` / `Cmd+Shift+D` split, `Cmd+M` magnify, `Cmd+W` close block, `Cmd+Shift+W` close tab, `Ctrl+Shift+{arrows,hjkl}` focus navigation (`frontend/app/store/keymodel.ts:520-600`). Adopted where Zed's keymap permits (§6.13).
8. **Header truncation** degrades to `irfansaf@Saf-M...`, hiding the process. Improved (§6.9).
9. **No left file explorer.** Wave uses a "files" block inside the grid instead. The sketch's persistent left sidebar is Zed's project panel — already shipped, zero work.

---

## 15. Implementation notes (Phase 1)

Written during implementation. Where the built behavior differs from the spec above, **this section wins** — each entry says what changed and why.

### 15.1 Keymap: most of §6.13 turned out to be unnecessary

The spec assumed we would rebind keys in a new `TerminalTile` context and fight Zed's context specificity. We do not need to. A tile is a `Pane`, and `Pane` already emits the events we care about:

| Key | Already bound to | What now happens |
|---|---|---|
| `cmd-d`, `ctrl-alt-<arrow>` | `pane::SplitRight` etc. in Terminal context | The tile pane emits `pane::Event::Split { direction }`; the group intercepts it and runs the guarded tile split. **No keymap change.** |
| `cmd-w` | `pane::CloseActiveItem` | Closes the tile's only item, which empties the pane, which emits `pane::Event::Remove`; the group removes the tile and collapses the axis. **No keymap change, and no `cmd_w_closes` setting is needed** — the Zed-native binding already produces the Wave-native behavior. |

Only two things needed new bindings, both purely additive in a new `TerminalGroup` context: `shift-escape` for magnify and `ctrl-shift-<arrow>` / `hjkl` for focus navigation. Added to all three platform keymaps.

Consequence: §6.13's contentious rows — the `cmd-shift-d` override of `debug_panel::ToggleFocus` and the `cmd-w` inversion — **do not exist**. Q1 in §11 is moot; the debug panel keeps its binding.

### 15.2 Actions live in a `terminal_group` namespace

`workspace::NewTerminalGroup` would have meant defining the action inside `workspace.rs`. Actions are instead `terminal_group::{New, SplitRight, …, ToggleMagnify}`, matching Zed's per-crate namespacing.

The tab-bar menu entry still works without a crate dependency: `pane.rs` resolves the action by name via `cx.build_action("terminal_group::New", None)`, falling back to omitting the entry if the crate is absent.

### 15.3 New rule: magnification follows focus

§6.6 did not say what happens when focus moves while a tile is magnified. Clearing the magnification and leaving focus somewhere invisible is incoherent, and so is keeping a magnified tile that no longer has focus.

**Rule:** while a group has a magnified tile, that tile *is* the focused tile. Moving focus moves the magnified view with it. `shift-escape` or a backdrop click is the way out.

This also collapses the stored state: `focused_tile` and `magnified_tile` are necessarily the same index when magnified, so no impossible layout can be persisted.

### 15.4 The guard permits when it cannot measure, and a hard cap backstops it

A tile created by a split has no measured bounds until the next paint, and `bounding_box_for_pane` returns `None` for a single-tile group by construction. Refusing on "unknown" blocked legitimate splits.

Resolved with three layers:
1. **Measure from the terminal itself.** A live terminal knows its own painted pixel size, which works even when the pane tree has no bounding box.
2. **Predict.** A split records what its two halves will measure, so a second split issued before the next paint is still judged against real geometry.
3. **Cap.** `MAX_TILES = 32` needs no measurement at all, so an unpainted group can never be split without bound.

Only when all three are unavailable — a group that has never painted — does a split proceed unmeasured, and the next one is governed.

### 15.5 Sibling proportions are preserved without changing Zed's pane behavior

`PaneAxis::insert_pane` re-equalizes the whole axis. Changing it would alter *editor* pane splitting too, which is out of scope. Instead the group snapshots the axis flexes before the split and rewrites them after, rescaling by `(n+1)/n` so every untouched sibling keeps its exact share and the split tile's share is halved between the two halves. `PaneAxis::flexes` is already public, so this needs no core change.

### 15.6 Two bugs found and fixed outside the feature

**`PaneAxis` bounding-box invariant (upstream Zed).** `insert_pane` and `remove` maintain `flexes` alongside `members` but never `bounding_boxes`, so the parallel array goes out of sync until the next layout pass. `bounding_box_for_pane` has a `debug_assert!` on exactly that invariant, so querying bounds between a structural change and the next paint panics in debug builds. Fixed by maintaining `bounding_boxes` in the same two places `flexes` is maintained. 263 workspace tests pass unchanged.

**`remote_server` build script (this fork).** `crates/remote_server/build.rs` still did `include_str!("../zed/Cargo.toml")` after the Zed→Rdg rename, which broke `./script/clippy` for the entire workspace — so ship gate 6 could not be evaluated at all. Repointed at `../rdg/Cargo.toml`.

> Superseded: `main` fixed this independently before the branch merged, so the change is no longer part of this work. Recorded because it blocked the lint gate while these phases were being built.

### 15.7 A crash the tests caught before the app ran it

`TerminalGroup::deploy` runs inside a workspace action handler, i.e. while the `Workspace` entity is already being updated. Reading the workspace synchronously from there (`database_id()`) panics with "cannot read Workspace while it is already being updated". Moved into the async block, which runs after the update completes.

Related: a tile spawns a shell when first focused, which raced with the explicit initial spawn and gave one tile two terminals. Tracked with an in-flight set, cleared by a `defer` guard so a failed spawn does not leave a tile unable to retry.

A third, found by reading the render path rather than by a test: the magnified tile was rendered **twice in one frame** — once inside the grid and again in the overlay. `PaneGroup::render` takes a `zoomed` pane to omit, which exists for exactly this; it was being passed `None`. The test suite did not catch this, because asserting "this pane appears once in the element tree" is not something the harness can express. It is the clearest example of why §15.9's hand-verification gap matters.

### 15.8 Merge-risk budget: spent 18 of 150 lines

| File | Added | Removed | What |
|---|---|---|---|
| `crates/workspace/src/pane.rs` | 10 | 3 | "New Terminal Group" tab-bar menu entry, resolved by action name |
| `crates/workspace/src/pane_group.rs` | 8 | 0 | The bounding-box invariant fix (§15.6) |

Everything else is the new `terminal_group` crate, additive keymap blocks, and app wiring.

### 15.9 Status against the Phase 1 exit criteria

| Item | State |
|---|---|
| `TerminalGroup` item in the center tab strip | Done |
| Tile tree, one terminal per tile (TG-1) | Done, enforced and tested |
| Split / close / focus / resize | Done — resize comes free from `PaneAxis` gutters |
| Split guard | Done, three-layer (§15.4) |
| Slim tile headers | Done — status dot, resolved title, magnify, close |
| Layout persistence | Done — shape, flexes, focus, magnify, title; layout-only by decision |
| Deferred shell spawn | Done — restore starts one shell, not N |
| Keymap | Done (§15.1) |
| Magnify | Done — brought forward from Phase 2 |
| Settings (`terminal_workspace.*`) | Done — `gap`, `corner_radius`, `min_tile_columns`, `min_tile_rows`, `split_guard`, `magnify_size`, `max_tiles`, live-reloadable |
| Drag to rearrange | Done — see §16 |

**Tests: 42 passing** — 8 pure guard tests, 17 integration tests including two that render at real window sizes. Ship gate 4 is covered twice: once as pure arithmetic and once against a laid-out 1400x900 grid.

**Not yet verified:** the feature has not been exercised by hand in a running editor. An attempt to drive the built app under automation was declined, so every claim above rests on the automated tests.


---

## 16. Implementation notes (Phase 2)

### 16.1 Drag to rearrange

All three drop semantics from §6.7 are implemented, and each maps onto an operation `PaneGroup` already provides — so the tree manipulation itself is code Zed exercises elsewhere:

| Zone | Region | Operation |
|---|---|---|
| **Swap** | Inner 50% × 50% of the target | `PaneGroup::swap` |
| **Edge** | Outer 25% band on each side | `remove` the dragged tile, then `split` beside the target |
| **Outer** | Within 24 px of the group's bounds | `PaneGroup::move_to_border` — which already existed for the Vim-style `ctrl-w shift-hjkl` motion |

Geometry lives in `drag.rs` as pure arithmetic on pixel rectangles, so all of it is tested without a window: 10 tests covering each edge, corner resolution (the nearer edge wins), the outer band beating the tile's own edge band, and every preview rectangle.

Behaviors beyond Wave:

- **The guard applies to drops.** An edge drop halves the target, so it answers to the same usability guard as a split. Wave permits the drop unconditionally and will produce an unusable tile.
- **`Escape` cancels an in-flight drag.** Zed's terminal binds bare `escape` to `SendKeystroke`, so this needed an explicit `menu::Cancel` handler on the group; without it a drag could only be ended by dropping it somewhere.
- **A self-drop is a no-op**, not a self-swap.
- **The stale drop target is cleared in `render`** rather than in a handler, because a drag that ends by cancellation or by release outside the group leaves no event on the element.

### 16.2 Keyboard equivalents

`terminal_group::{SwapLeft, SwapRight, SwapUp, SwapDown}` on `ctrl-alt-shift-<arrow>` — the keyboard form of dragging a tile onto another's centre, and the accessibility path required by E20. `terminal_group::Equalize` restores equal shares via `PaneGroup::reset_pane_sizes`.

### 16.3 Status

**Tests: 42 passing** — 8 split-guard, 10 drag-geometry, 5 persistence, 19 integration.

| Phase 2 item | State |
|---|---|
| Drag with all three drop zones | Done |
| Live filled preview | Done |
| Guard applied to drops | Done |
| Escape cancels | Done |
| Swap bindings | Done |
| Equalize | Done |
| Magnify | Done in Phase 1 |

See §17 — this has now been verified by hand.


---

## 17. Hand verification (2026-09-01)

Driven directly in a running build. **Everything in Phases 1 and 2 works**, and the session found four bugs that 42 passing tests did not.

### 17.1 Confirmed working

| Behavior | Evidence |
|---|---|
| `terminal group: new` opens a grid tab | Tab appears in the center strip with a terminal icon |
| Slim tile header | Status dot, resolved title (`rdg-grid-test — zsh`), magnify, close |
| Live shells, independent PTYs | ttys016 / 018 / 019 / 020 / 025 |
| `cmd-d` splits the **tile**, not the workspace pane | Retargeted with zero keymap change, as designed in §15.1 |
| Tab title tracks tile count | "Terminal (2)" → "Terminal (5)" |
| New tile takes focus | Cursor moves to the new tile |
| **Guard adapts** | With no width left, the column split **downward** instead |
| **Guard refuses** | Four further `cmd-d` produced **zero** tiles, with the specified toast |
| **Sibling proportions preserved** | The first tile stayed wide while others subdivided — the behavior Wave destroys |
| Magnify | Tile floats over a dimmed, still-visible grid; renders once |
| Focus navigation | `ctrl-shift-←` moved focus, proven with typed markers |
| Drag: preview card + all three zones | Swap = whole tile, Edge = half, Outer = full-height band |
| Drop commits | A tile promoted to a full-height column at the root axis |
| **Layout persistence** | The exact tree shape returned after a full restart, promoted column included |
| Magnify state persistence | The magnified tile came back magnified |
| Deferred spawn | Restore started one shell; the rest followed, staggered |

**Ship gate 4 is met in the real app.** The Wave failure reproduced in §14 — seven ~48 px tiles with unreadable prompts — does not occur.

### 17.2 Four bugs found by hand, none catchable by the suite

| Bug | Root cause | Why no test caught it |
|---|---|---|
| **`shift-escape` zoomed the whole pane instead of magnifying the tile** | Zed's *first* keymap block has no `"context"` key, making `workspace::ToggleZoom` global. A context-less binding matches at the **deepest** node in the dispatch path — the terminal itself — so it won before the walk reached the `TerminalGroup` node. Fixed by claiming the key at that same node with a `"TerminalGroup > Terminal"` context. | The harness cannot model Zed's keymap dispatch tree. `ctrl-shift-<arrow>` worked throughout, because nothing global competes for it — which is exactly what made this invisible. |
| **Restored tiles rendered no header** | `Pane::render` gates the tab bar on `active_item().is_some()`, so a tile awaiting its deferred shell was a blank box with no title and no close button. | The tests assert tile counts and tree shape, never "this tile drew a header". |
| **Restored shells started in the process working directory**, not the project root | `create_terminal_shell(None)` falls back to the process cwd. Now passes the project's first visible worktree, read from the project rather than the workspace to stay clear of the re-entrancy trap in §15.7. | The test project is a `FakeFs` with no worktree, so the fallback never showed. |
| **Magnified tile rendered twice per frame** (found in Phase 1 by reading the render path) | `PaneGroup::render` takes a `zoomed` pane to omit; it was passed `None`. | "This pane appears once in the element tree" is not expressible in the harness. |

The deferred-spawn design changed as a result: restore starts the focused tile immediately, then brings the rest up **sequentially** after 500 ms, rather than waiting for each to be focused. A grid of blank headerless boxes is worse than forking a few processes.

### 17.3 What this says about the test strategy

The suite is good at what it can reach — tree manipulation, the guard's arithmetic, persistence round-trips, invariants. It is blind to three things, and all three broke:

1. **Keymap dispatch** — context precedence is resolved by the app at runtime.
2. **Whether an element actually drew** — headers, single-render, preview placement.
3. **Environment fallbacks** — anything the fake test project does not model.

Any future phase should assume these need a hand pass, and budget for one.


### 17.4 Reversal: the guard must not refuse a move

Found by dragging a tile in a dense grid and being told *"Not enough room for another terminal."*

§6.7 and §16.1 originally said the split guard applies to drops. **That was wrong, and the reasoning was wrong.** A drop *relocates* an existing tile: the count does not change and the source vacates its old space. Refusing it prevents nothing — no terminal is created — while leaving a dense grid impossible to rearrange. The guard exists to stop a tile being *born* unusable, not to stop the user moving one they already have and can drag straight back.

**New rule:** on a drop, the guard picks the better axis when the requested one is cramped, and otherwise honors the user's intent. It never refuses. Splits are unchanged — there the guard still refuses, because a split does create a new terminal.

A second bug was hiding under the first: when the guard adapted a drop's direction, the adapted value was discarded and the insert used the original direction anyway. A drop that survived the guard could still land on the wrong axis.

Worth recording *why* the refusal fired at all, because the numbers are not obvious: the target tile was roughly 480x340pt. Halving it downward yields 7 rows against a floor of 8; halving it rightward yields 29 columns against a floor of 30. Both missed by a hair, `Adapt` had nowhere to go, and the drop was rejected. The thresholds are defensible for a split; applying them to a move was the mistake.


### 17.5 The grid had no mouse affordance for adding a tile

Found by asking the obvious question: *with a group open, how do I add a terminal to it by clicking?*

There was no answer. `cmd-d` worked, but the tab bar's `+` belongs to the **workspace pane**, so the remaining New Terminal action creates something *outside* the group. A mouse-only user could open a grid and never grow it. it.

**Fixed:** every tile header now carries a `+` button that adds a tile beside it, routed through the same guarded split as the keyboard. It sits left of magnify and close, so the three tile actions read as one set.

### 17.6 Dragging a terminal tab into a group — resolved with no core change

Dragging an existing terminal tab onto the grid reorders the tab strip instead of adding a tile.

The cause is structural, not a missing handler. During a `DraggedTab` drag, the **pane hosting the group renders its own drop overlay above the group's content**, so the drop never reaches the group — a handler on the group's root, even one calling `stop_propagation`, is never invoked. This was verified by building it and watching the drop still land on the pane.

The half-built version was **removed rather than left in**: it painted a drop preview over the target tile and then did nothing, which is worse than no affordance at all.

**The fix needed no change to `pane.rs` at all.** `Pane::handle_tab_drop` already offers every drop to its active item first:

```rust
if is_pane_target && ix == self.active_item_index
    && let Some(active_item) = self.active_item()
    && active_item.handle_drop(self, dragged_tab, window, cx) { return; }
```

`Item::handle_drop` had simply never been implemented on `TerminalGroup`. Implementing it consumes the drop and places the terminal in a tile of its own. The workspace footprint stayed at 18 added / 3 removed — the earlier plan to teach `pane.rs` about terminal groups would have been the wrong change, coupling a core file to a downstream crate for a seam that already existed.

Three details that make the implementation sound:

- **The move is deferred.** `handle_drop` runs inside the update of both the source pane and the group, and the move has to touch both again. `window.defer` lets the current update finish first. The item is re-located by id inside the deferred closure rather than captured by index, because the source pane may have changed in between.
- **`weak_self`.** `Item::handle_drop` is handed `&self` and a bare `App`, with no context to reach the entity through, so the group keeps a weak handle to itself.
- **Arriving terminals are guarded like splits, not like moves.** §17.4 exempted internal rearranges because they do not change the tile count. A terminal arriving from *outside* does add a tile, so it answers to the guard. When there is no room the drop is declined by returning `false`, and the pane files it as a sibling tab: the terminal is never lost, it simply does not join a full grid.

There are now four ways into a group: the header `+`, `cmd-d`, the command palette, and dragging a terminal tab onto the grid.
