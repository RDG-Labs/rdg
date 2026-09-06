---
title: Terminal Groups - Rdg
description: Rdg's terminal-first workspace for running multiple shells and coding agents alongside the editor.
---

# Terminal

Rdg treats terminals as first-class workspace items. Use terminal groups to tile multiple shells and coding agents alongside the editor.

## Opening Terminals

| Action                | macOS        | Linux/Windows |
| --------------------- | ------------ | ------------- |
| Open terminal group   | `` Ctrl+` `` | `` Ctrl+` ``  |
| Open new terminal     | `Ctrl+~`     | `Ctrl+~`      |

Use {#action terminal_group::New} to open a terminal group as a regular workspace tab. Use {#action workspace::NewTerminal} to open a terminal in the active pane.

### Terminal Groups

A terminal group is a regular workspace tab containing one or more tiled terminals. Create additional groups with `` Ctrl+` `` and split or move between terminals using the terminal-group actions.

## Working with Multiple Terminals

Create additional terminals with the `+` button in a tile header or split a focused tile. Each terminal gets its own tile and can run an independent shell or coding agent.

Use the arrow or `h`/`j`/`k`/`l` bindings to move focus between tiles. Use `Ctrl+Tab` to cycle through tiles, `Ctrl+W` (`Cmd+W` on macOS) to close the focused tile, and `Ctrl+Shift+0` (`Cmd+Shift+0` on macOS) to equalize the layout.

## Configuring the Shell

By default, Rdg uses your system's default shell (from `/etc/passwd` on Unix systems). To use a different shell:

```json [settings]
{
  "terminal": {
    "shell": {
      "program": "/bin/zsh"
    }
  }
}
```

To pass arguments to your shell:

```json [settings]
{
  "terminal": {
    "shell": {
      "with_arguments": {
        "program": "/bin/bash",
        "args": ["--login"]
      }
    }
  }
}
```

## Working Directory

Control where new terminals start:

| Value                                         | Behavior                                                                                                          |
| --------------------------------------------- | ----------------------------------------------------------------------------------------------------------------- |
| `"current_file_directory"`                    | Uses the current file's directory, falling back to the project directory, then the first project in the workspace |
| `"current_project_directory"`                 | Uses the current file's project directory (default)                                                               |
| `"first_project_directory"`                   | Uses the first project in your workspace                                                                          |
| `"always_home"`                               | Always starts in your home directory                                                                              |
| `{ "always": { "directory": "~/projects" } }` | Always starts in a specific directory                                                                             |

```json [settings]
{
  "terminal": {
    "working_directory": "first_project_directory"
  }
}
```

## Environment Variables

Add environment variables to all terminal sessions:

```json [settings]
{
  "terminal": {
    "env": {
      "EDITOR": "rdg --wait",
      "MY_VAR": "value"
    }
  }
}
```

> **Tip:** Use `:` to separate multiple values in a single variable: `"PATH": "/custom/path:$PATH"`

### Python Virtual Environment Detection

Rdg can automatically activate Python virtual environments when opening a terminal. By default, it searches for `.env`, `env`, `.venv`, and `venv` directories:

```json [settings]
{
  "terminal": {
    "detect_venv": {
      "on": {
        "directories": [".venv", "venv"],
        "activate_script": "default"
      }
    }
  }
}
```

The `activate_script` option supports `"default"`, `"csh"`, `"fish"`, and `"nushell"`.

To disable virtual environment detection:

```json [settings]
{
  "terminal": {
    "detect_venv": "off"
  }
}
```

## Fonts and Appearance

The terminal can use different fonts from the editor:

```json [settings]
{
  "terminal": {
    "font_family": "JetBrains Mono",
    "font_size": 14,
    "font_features": {
      "calt": false
    },
    "line_height": "comfortable"
  }
}
```

Line height options:

- `"comfortable"` — 1.618 ratio, good for reading (default)
- `"standard"` — 1.3 ratio, better for TUI applications with box-drawing characters
- `{ "custom": 1.5 }` — Custom ratio

### Cursor

Configure cursor appearance:

```json [settings]
{
  "terminal": {
    "cursor_shape": "bar",
    "blinking": "on"
  }
}
```

Cursor shapes: `"block"`, `"bar"`, `"underline"`, `"hollow"`

Blinking options: `"off"`, `"terminal_controlled"` (default), `"on"`

### Minimum Contrast

Rdg adjusts terminal colors to maintain readability. The default value of `45` ensures text remains visible. Set to `0` to disable contrast adjustment and use exact theme colors:

```json [settings]
{
  "terminal": {
    "minimum_contrast": 0
  }
}
```

## Scrolling

Navigate terminal history with these keybindings:

| Action           | macOS                          | Linux/Windows    |
| ---------------- | ------------------------------ | ---------------- |
| Scroll page up   | `Shift+PageUp` or `Cmd+Up`     | `Shift+PageUp`   |
| Scroll page down | `Shift+PageDown` or `Cmd+Down` | `Shift+PageDown` |
| Scroll line up   | `Shift+Up`                     | `Shift+Up`       |
| Scroll line down | `Shift+Down`                   | `Shift+Down`     |
| Scroll to top    | `Shift+Home` or `Cmd+Home`     | `Shift+Home`     |
| Scroll to bottom | `Shift+End` or `Cmd+End`       | `Shift+End`      |

Adjust scroll speed with:

```json [settings]
{
  "terminal": {
    "scroll_multiplier": 3.0
  }
}
```

## Copy and Paste

| Action | macOS   | Linux/Windows  |
| ------ | ------- | -------------- |
| Copy   | `Cmd+C` | `Ctrl+Shift+C` |
| Paste  | `Cmd+V` | `Ctrl+Shift+V` |

### Copy on Select

Automatically copy selected text to the clipboard:

```json [settings]
{
  "terminal": {
    "copy_on_select": true
  }
}
```

### Keep Selection After Copy

By default, text stays selected after copying. To clear the selection:

```json [settings]
{
  "terminal": {
    "keep_selection_on_copy": false
  }
}
```

## Search

Search terminal content with `Cmd+F` (macOS) or `Ctrl+Shift+F` (Linux/Windows). This opens the same search bar used in the editor.

## Vi Mode

Toggle vi-style navigation in the terminal with `Ctrl+Shift+Space`. This allows you to navigate and select text using vi keybindings.

## Clear Terminal

Clear the terminal screen:

- macOS: `Cmd+K`
- Linux/Windows: `Ctrl+Shift+L`

## Option as Meta (macOS)

For Emacs users or applications that use Meta key combinations, enable Option as Meta:

```json [settings]
{
  "terminal": {
    "option_as_meta": true
  }
}
```

This reinterprets the Option key as Meta, allowing sequences like `Alt+X` to work correctly.

## Alternate Scroll Mode

When enabled, mouse scroll events are converted to arrow key presses in applications like `vim` or `less`:

```json [settings]
{
  "terminal": {
    "alternate_scroll": "on"
  }
}
```

## Path Hyperlinks

Rdg detects file paths in terminal output and makes them clickable. `Cmd+Click` (macOS) or `Ctrl+Click` (Linux/Windows) opens the file in Rdg, jumping to the line number if one is detected.

Common formats recognized:

- `src/main.rs:42` — Opens at line 42
- `src/main.rs:42:10` — Opens at line 42, column 10
- `File "script.py", line 10` — Python tracebacks

By default, `Cmd+Click`/`Ctrl+Click` opens links even when the running application has enabled mouse reporting (e.g. vim with `mouse=a`, htop). If you prefer those clicks to be forwarded to the application instead, disable `open_links_in_mouse_mode`; links can then still be opened with `Shift+Cmd+Click` (`Shift+Ctrl+Click`):

```json
{
  "terminal": {
    "open_links_in_mouse_mode": false
  }
}
```

## Panel Configuration

### Dock Position

```json [settings]
{
  "terminal": {
    "dock": "bottom"
  }
}
```

Options: `"bottom"` (default), `"left"`, `"right"`

### Default Size

```json [settings]
{
  "terminal": {
    "default_width": 640,
    "default_height": 320
  }
}
```

### Toolbar

Show the terminal title in a breadcrumb toolbar:

```json [settings]
{
  "terminal": {
    "toolbar": {
      "breadcrumbs": true
    }
  }
}
```

The title can be set by your shell using the escape sequence `\e]2;Title\007`.

## Integration with Tasks

The terminal integrates with Rdg's [task system](./tasks.md). When you run a task, it executes in the terminal. Rerun the last task from a terminal with:

- macOS: `Cmd+Alt+R`
- Linux/Windows: `Ctrl+Shift+R` or `Alt+T`

## AI Assistance

Get help with terminal commands using the [Inline Assistant](./ai/inline-assistant.md):

- macOS: `Ctrl+Enter`
- Linux/Windows: `Ctrl+Enter` or `Ctrl+I`

This opens the Inline Assistant to help explain errors, suggest commands, or troubleshoot issues. AI agents in the [Agent Panel](./ai/agent-panel.md) can also run terminal commands as part of their workflow.

## Sending Text and Keystrokes

For advanced keybinding customization, you can send raw text or keystrokes to the terminal:

```json [keymap]
{
  "context": "Terminal",
  "bindings": {
    "alt-left": ["terminal::SendText", "\u001bb"],
    "ctrl-c": ["terminal::SendKeystroke", "ctrl-c"]
  }
}
```

## Running Coding Agents and CLI Workers

A terminal group is also the place to run coding agents and long-lived CLI
workers side by side with your editor. Launch an installed coding agent, or
spawn any command as a supervised worker, and control it from the same window
it runs in.

### Spawning a worker

- **Launch an installed coding agent** from a tile header `+` menu, or add it
  from the command palette.
- **Spawn a custom command** with `+ → Custom Command…` in a tile header.
- **Programmatically** via the orchestration CLI (below).

Each worker runs as a normal, visible terminal — never hidden, never filtered.

### Worker overflow

The visible grid has a limit (`max_tiles`, default 32) so tiles stay large
enough to read. When you spawn more workers than the grid can visibly fit, the
extra ones are **spilled to overflow**: they keep running as real terminals but
take no tile, and are listed in a bar under the grid and in **Mission Control**
(the list-tree icon).

This means you can coordinate far more agents than you could ever see at once —
the grid stays readable and the agents keep working.

### Mission Control

Open Mission Control to see every worker as a tree (parent/child), each with a
status marker and menu: **Focus** (surfaces an off-grid worker into a tile),
**Restart**, **Pause/Resume**, **Send to Overflow**, and **Close**.

| Marker | Status |
| --------- | ------------------ |
| `●`       | starting / working |
| `○`       | waiting            |
| `✓`       | completed          |
| `✕`       | failed             |

### Promote and demote

- **Promote** an off-grid worker onto the grid (Mission Control → Focus, or the
  arrow on its overflow row). Its tile appears without restarting the process —
  the same PTY keeps running.
- **Demote** a visible worker back to overflow (its header arrow, or Mission
  Control → Send to Overflow) to free a tile. The process keeps running.

Both moves preserve the live process and the worker's identity, so its status
and reporting keep working.

### Pause and resume

Pause a worker (its process receives `SIGSTOP`) from the tile header, the
overflow row, or Mission Control; resume it (`SIGCONT`) the same way. Useful
for a busy agent you want to hold while you act on its output.

### Surviving a restart

Workers spilled to overflow are **remembered**: on restart the grid is
restored and the spill set is re-created by its commands. Live processes
themselves don't survive a restart (Rdg hosts no background daemon), so a
worker picked up mid-run is re-started, not resumed.

### Orchestration CLI

Every worker inherits `RDG_GROUP_ID`, `RDG_WORKER_ID`, and (for children)
`RDG_PARENT_WORKER_ID`, plus the `rdg` control wrapper, so you can orchestrate
recursively from any worker's shell:

```bash
# Spawn a worker (capture its id)
worker_json="$(rdg --control spawn "pi")"
worker_id="$(printf '%s' "$worker_json" | python3 -c 'import json,sys; print(json.load(sys.stdin)["Spawned"]["worker_id"])')"

# Send it a task
rdg --control send "$worker_id" "Summarize the auth flow; report only"

# Inspect the tree
rdg --control list

# Broadcast a read-only instruction to all workers
rdg --control broadcast all "Pause and report your state"

# Report status from inside a worker
rdg --control report "$RDG_WORKER_ID" working "Inspecting the auth code"

# Watch lifecycle events
rdg --control watch
```

The full protocol (`spawn`, `send`, `broadcast`, `list`, `watch`, `report`,
`close`) is described by the RDG orchestration skill; install it with
`npx skills add RDG-Labs/rdg --skill rdg-orchestration` or the Terminal Group
`+ → Install RDG Orchestration Skill…` entry.

## A typical session

1. **Open a repository** and create a terminal group (`Ctrl+\``).
2. **Start services** that take a tile each: API server, frontend dev server,
   a watcher, a log tail.
3. **Launch agents** from a tile's `+` menu until the grid is full; any beyond
   it spill to overflow and stay supervised in Mission Control.
4. **Split and rearrange** tiles by dragging headers; magnify a tile you need
   to read closely.
5. **Send follow-up tasks** to an agent with `rdg --control send`.
6. **Act on failures** — a failed worker shows `✕`; use Mission Control to
   restart it or close it.
7. **Pause** a noisy agent while you work, then resume it.
8. **Restart Rdg** — the grid and the overflow set come back; start focused
   services where you left them.

## All Terminal Settings

For the complete list of terminal settings, see the [Terminal section in All Settings](./reference/all-settings.md#terminal).

## What's Next

- [Tasks](./tasks.md) — Run commands and scripts from Rdg
- [REPL](./repl.md) — Interactive code execution
- [CLI Reference](./reference/cli.md) — Command-line interface for opening files in Rdg
