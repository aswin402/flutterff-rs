# Usage and Examples

To run the launcher, navigate to your Flutter project root and simply execute:

```bash
flutterff-rs [OPTIONS]
```

## Command Line Arguments

| Argument     | Shortcut | Function                                                                    |
| :----------- | :------- | :-------------------------------------------------------------------------- |
| `--size`     | `-s`     | Device presets (`mobile`, `mobile-small`, `iphone`, `tablet`, `desktop`) or custom bounds (`WxH`) |
| `--port`     | `-p`     | Standard HTTP Port to serve on (default: `8080`). Auto-increments if in use. |
| `--offline`  |          | Disables `flutter pub get` and external script fetches for offline coding.  |
| `--profile`  |          | Runs flutter in profile mode.                                               |
| `--flavor`   |          | Specifies the web flavor setting.                                           |
| `--no-hot`   |          | Disables hot reload.                                                        |
| `--list-sizes`|         | Print the dimensions of the preset size options.                            |

## Practical Examples

**1. Testing standard mobile layout:**
```bash
flutterff-rs
```

**2. Specifying a custom dynamic size for testing responsive layouts:**
```bash
flutterff-rs --size 1080x1920
```

**3. Running without an internet connection:**
```bash
flutterff-rs --offline --port 3000
```

## Interactive UI Features

- **Size Switcher Picker:** The top-left of the GTK header bar lets you hot-swap window bounds to device presets instantly.
- **Screenshot capture 📸 :** Takes a reliable Cairo render directly from the GTK surface, saving an artifact to `screenshots/` at exactly the rendered Webview size.
- **Hot Reload ⚡:** The thunder icon simulates sending an `r` to Flutter.
- **Hot Restart:** The refresh icon forcefully re-renders the Webview, mimicking a robust Flutter hot restart safely natively.
