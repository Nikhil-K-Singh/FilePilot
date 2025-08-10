# FilePilot Configuration

FilePilot supports customizable key bindings through configuration files. You can modify these settings to match your preferred workflow.

## Configuration File Locations

FilePilot searches for configuration files in the following order:

1. **Specified with `--config`**: Use `filepilot --config /path/to/config.json`
2. **Development**: `src/config.json` (when running from project directory)
3. **Current directory**: `config.json`
4. **Local project**: `.filepilot/config.json`
5. **User home**: `~/.filepilot/config.json` (recommended for global use)
6. **Next to executable**: `config.json` in the same directory as the filepilot binary

## Quick Setup

### For Global Use
```bash
# Create default configuration in your home directory
filepilot --create-config

# This creates ~/.filepilot/config.json which you can then edit
```

### For Project-Specific Use
```bash
# Create config in current directory
filepilot --create-config
mv ~/.filepilot/config.json ./config.json
```

## Key Binding Configuration

### Structure

The key bindings are organized into categories:

- **navigation**: Basic movement keys (up, down, left, enter)
- **actions**: File operations (quit, search, open, reveal, share)
- **search_mode**: Keys active during search input
- **search_results**: Keys for navigating search results

### Example Configuration

```json
{
  "notification_endpoint": "http://localhost:5005/api/data",
  "notification_enabled": true,
  "key_bindings": {
    "navigation": {
      "up": ["Up", "k"],
      "down": ["Down", "j"],
      "left": ["Left", "h"],
      "enter": ["Enter"]
    },
    "actions": {
      "quit": ["q"],
      "search": ["/"],
      "open": ["o", "O"],
      "reveal": ["r", "R"],
      "share": ["s", "S"]
    },
    "search_mode": {
      "exit_search": ["Esc"],
      "exit_to_results": ["Enter"],
      "toggle_strategy": ["F2"],
      "navigate_tab": ["Tab"],
      "backspace": ["Backspace"]
    },
    "search_results": {
      "back": ["Esc", "Left"]
    }
  }
}
```

### Supported Key Names

#### Special Keys
- `"Up"`, `"Down"`, `"Left"`, `"Right"` - Arrow keys
- `"Enter"` - Enter/Return key
- `"Esc"` - Escape key
- `"Tab"` - Tab key
- `"Backspace"` - Backspace key
- `"F1"` through `"F12"` - Function keys

#### Character Keys
- Single characters: `"a"`, `"b"`, `"c"`, etc.
- Case-sensitive: `"A"` is different from `"a"`
- Special characters: `"/"`, `"?"`, `"."`, etc.

### Multiple Key Bindings

Each action can have multiple key bindings. For example:
```json
"up": ["Up", "k"]
```
This means both the Up arrow key and the "k" key will move up.

### Vim-style Navigation

The default configuration includes vim-style navigation:
- `k` - Move up (same as Up arrow)
- `j` - Move down (same as Down arrow)  
- `h` - Move left/go up directory (same as Left arrow)

### Custom Examples

#### Emacs-style Navigation
```json
"navigation": {
  "up": ["Up", "C-p"],
  "down": ["Down", "C-n"],
  "left": ["Left", "C-b"],
  "enter": ["Enter", "C-m"]
}
```

#### Gaming-style (WASD)
```json
"navigation": {
  "up": ["Up", "w"],
  "down": ["Down", "s"],
  "left": ["Left", "a"],
  "enter": ["Enter", "d"]
}
```

#### Custom Actions
```json
"actions": {
  "quit": ["q", "Q", "x"],
  "search": ["/", "f"],
  "open": ["o", "Enter"],
  "reveal": ["r", "v"],
  "share": ["s", "u"]
}
```

## Configuration Tips

1. **Backup**: Always backup your working configuration before making changes
2. **Testing**: Test your changes immediately to ensure they work as expected
3. **Conflicts**: Avoid assigning the same key to multiple actions
4. **Accessibility**: Consider accessibility when choosing key combinations
5. **Muscle Memory**: Choose keys that match your existing workflows

## Troubleshooting

If you encounter issues:

1. **Invalid JSON**: Ensure your JSON syntax is correct
2. **Unknown Keys**: Check that all key names are spelled correctly
3. **Fallback**: Delete the config file to restore defaults
4. **Case Sensitivity**: Remember that "a" and "A" are different keys

## Default Key Bindings

If no configuration file is found, FilePilot uses these defaults:

| Action | Keys | Description |
|--------|------|-------------|
| Move Up | ↑, k | Navigate up in file list |
| Move Down | ↓, j | Navigate down in file list |
| Go Back | ←, h | Go to parent directory |
| Open/Navigate | Enter | Open file or enter directory |
| Quit | q | Exit FilePilot |
| Search | / | Enter search mode |
| Open | o, O | Open file with default application |
| Reveal | r, R | Show file in file manager |
| Share | s, S | Share file via web server |

## Need Help?

If you need assistance with configuration, please check the project documentation or open an issue on the GitHub repository.
