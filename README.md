# Orbassist

A lightweight desktop utility that floats above all windows. Move the orb anywhere and pin it to screen edges. Click to open a radial menu with quick shortcuts to applications, folders, websites, keyboard shortcuts, and system controls.

Inspired by AssistiveTouch (iOS) and Quick Ball (Xiaomi), Orb provides a modern Windows alternative without the jank. The orb remains accessible even when transparent via hotkey and opacity settings.

## Features
- Floating orb that stays on top of all windows
- Pin orb to screen edges
- Radial menu with application/folder/URL shortcuts
- Global hotkey to show menu at cursor position
- Customizable opacity and size
- Theme support (light/dark)
- Optional custom images for the orb, tray icon, and menu slots

## Configuration

Orb stores its settings in `%APPDATA%\Orb\config.toml` (for example `C:\Users\<you>\AppData\Roaming\Orb\config.toml`). Open the settings window from the tray icon (**Pengaturan**); saving there rewrites this file without restarting Orb.

## Custom images (optional)

Drop image files into the assets folder:

```
%APPDATA%\Orb\assets
```

That is `C:\Users\<you>\AppData\Roaming\Orb\assets` for a typical install. Orb creates the folder automatically, and the **Buka folder aset** button in the settings window opens it for you.

| File | Used for | If missing |
|---|---|---|
| `ball.png` | The floating orb image | Built-in orb drawing |
| `tray.png` | System tray icon | Icon generated from the orb |
| anything, e.g. `chrome.png` | Menu slot icon (type its filename in the slot's "Ikon" column) | First letter of the slot label |

Supported extensions: `.png`, `.ico`, `.jpg`, `.jpeg`, `.bmp`. Images are scaled to fit.

Nothing is required: with an empty folder Orb uses its built-in look. A file that cannot be read is logged and skipped, so Orb still starts.

You can point elsewhere with an optional config section (empty values keep the defaults above):

```toml
[assets]
dir = "D:\\orb icons"   # empty = %APPDATA%\Orb\assets
ball = "bola.png"       # empty = auto-find ball.*
tray = "tray.ico"       # empty = auto-find tray.*, else the orb icon
```
