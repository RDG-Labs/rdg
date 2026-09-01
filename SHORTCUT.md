# Rdg shortcuts

Rdg supports familiar shortcut profiles from Zed, VS Code, JetBrains, Sublime Text, Atom, TextMate, Emacs, and Cursor. Use `"None"` to disable base bindings.

## Choose a shortcut profile

Open the command palette and run **Toggle Base Keymap**, or add one of these to `settings.json`:

```json
{
  "base_keymap": "VSCode"
}
```

Use `"JetBrains"` for IntelliJ-style shortcuts. `"Zed"` is the default.

## Quick controls

| Action | macOS | Windows/Linux |
| --- | --- | --- |
| Command palette | `Cmd+Shift+P` | `Ctrl+Shift+P` |
| Keymap editor | `Cmd+K Cmd+S` | `Ctrl+K Ctrl+S` |
| Settings | `Cmd+,` | `Ctrl+,` |
| Settings file | `Cmd+Alt+,` | `Ctrl+Alt+,` |
| New terminal | `Ctrl` + backtick | `Ctrl+Shift` + backtick |

The keymap editor shows every available action and lets you change bindings without editing JSON. Custom bindings are stored in `keymap.json` and override the selected base keymap.
