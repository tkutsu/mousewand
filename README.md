# mousewand

Mouse gestures for Linux. Hold a mouse button, draw a stroke, and the window under the cursor gets the action you set for that stroke in that app.

![Drawing a stroke to the left in Firefox goes back a page, and a stroke to the right goes forward again. Each leaves a short orange trail, and a hint at the bottom names the gesture.](docs/demo.gif)

It runs on Hyprland, KDE Plasma 6, GNOME, sway, niri, COSMIC, labwc and X11 desktops, and picks the right one on its own. A quick click of the trigger button is still a click, so middle-click paste keeps working, and holding the button still is still a drag.

## Install

Build it with a recent [Rust](https://rustup.rs) toolchain:

```fish
git clone https://github.com/tkutsu/mousewand
cd mousewand
cargo install --path crates/mousewand
```

mousewand reads your mice directly and passes their events on through a virtual mouse. This udev rule lets the logged-in user do that without root:

```fish
sudo cp dist/70-mousewand.rules /etc/udev/rules.d/
sudo cp dist/mousewand-uinput.conf /etc/modules-load.d/
sudo modprobe uinput; and sudo udevadm control --reload; and sudo udevadm trigger
```

Log out and back in once. Being in the `input` group works too.

Run `mousewand`, hold the middle button and draw. Ctrl+C stops it and gives the mouse back.

To start it with your desktop, add `mousewand` to your compositor's autostart, such as `exec-once` in Hyprland, or use the systemd user service:

```fish
mkdir -p ~/.config/systemd/user
cp dist/mousewand.service ~/.config/systemd/user/
systemctl --user enable --now mousewand
```

Only one mousewand runs at a time, so doing both does no harm.

### Settings app

The settings app is optional. It's a Tauri app and needs WebKitGTK (`webkit2gtk-4.1`), Node and pnpm to build:

```fish
pnpm -C crates/mousewand-settings/ui install
pnpm -C crates/mousewand-settings/ui build
cargo install --path crates/mousewand-settings
```

Draw a circle over a window to open it on that window's app. `mousewand-settings --class firefox` opens it from a terminal. It edits the same files described below, keeps your comments, and won't save a config mousewand would reject. It can also record a shortcut or detect your trigger button as you press them, and teach mousewand new shapes.

## Configuration

```
~/.config/mousewand/
  config.toml         settings, gestures for every app, global gestures, hints
  apps/<class>.toml   gestures for one app, by window class
  shapes/<name>.toml  shapes drawn in the settings app
  themes/<name>.toml  trail themes
```

Changes apply when you save. If a file has an error, mousewand keeps the last working config and shows the error in a notification.

The examples set up Firefox, kitty, volume on the wheel, an alt-tab and hints. Their two workspace gestures use Hyprland dispatchers.

```fish
mkdir -p ~/.config/mousewand
cp -r examples/* ~/.config/mousewand/
```

### Gestures

A gesture is an input and an action, with an optional name for hints to show:

```toml
# ~/.config/mousewand/apps/firefox.toml
match = ["firefox", "librewolf"]

[[gesture]]
stroke = "L"
name = "Back"
action = { keys = "alt+Left" }
```

Without `match`, an app file applies to the window class in its file name. An app's gestures replace the ones in `config.toml` that have the same input.

The input is one of these:

- `stroke = "DR"`: directions in the order you draw them. `U`, `D`, `L` and `R`, and diagonals as numpad digits: `7` up-left, `9` up-right, `1` down-left, `3` down-right.
- `shape = "circle"`: a drawn shape. See [Shapes](#shapes).
- `wheel = "up"`: a wheel notch while you hold the trigger, `up`, `down`, `left` or `right`. It fires on every notch.
- `click = "right"`: a `left` or `right` click while you hold the trigger.

The action is one of these:

- `keys = "ctrl+shift+t"`: a key combo for the window you drew over. Keys are positions on a US keyboard, whatever your layout.
- `command = "..."`: a shell command. It gets `$MOUSEWAND_CLASS` and `$MOUSEWAND_TITLE`.
- `dispatch = "..."`: a compositor command. See [Desktops](#desktops) for what each one takes.

A stroke can keep firing while you hold the trigger. With `repeat = true` it fires when it's recognized, again for every `step` pixels (80 by default) you keep moving in its last direction, and on every wheel notch, which suits scrubbing through tabs. With `hold` in the action it does the same and holds those keys down until you let go, which makes a real alt-tab:

```toml
[[gesture]]
stroke = "9"
name = "Switch windows"
action = { keys = "Tab", hold = "alt" }
```

A repeating stroke takes over as soon as it's drawn, so no other stroke may start with it. mousewand rejects a config where one does.

### Shapes

A shape matches by how the drawing looks rather than its directions. A circle counts when it's squashed, tilted, drawn either way round, or not quite closed.

`circle` is built in. To add a shape, draw it five times in the settings app. It saves the shape as `shapes/<name>.toml` and lists every shape under General, where you can redraw or remove them.

```toml
[[gesture]]
shape = "circle"
name = "Reopen tab"
action = { keys = "ctrl+shift+t" }
```

Shapes fire on release and can't repeat or hold keys. A stroke with a gesture of its own takes precedence over a shape, so a square drawn as `LDRU` stays a square if `LDRU` has a gesture. Otherwise it's round enough to count as a circle.

### Global gestures

Gestures under `[global]` are the same in every app, and app files can't replace them. With a `trigger` of their own, that button runs only them, and the main trigger keeps the app gestures:

```toml
[global]
trigger = "back"     # optional; without it they share the main trigger

[[global.gesture]]
stroke = "RU"
name = "Next workspace"
action = { dispatch = 'hl.dsp.focus({ workspace = "e+1" })' }
```

On their own button, `keys` go to the focused window rather than the one under the cursor.

### Settings

These are the defaults:

```toml
trigger = "middle"   # middle, back (mouse 4), forward (mouse 5), mouse6 to mouse8
threshold = 8        # pixels to move before a press becomes a gesture
hold_ms = 300        # holding still this long makes it a normal press
min_segment = 30     # shorter strokes are ignored as noise
devices = []         # only use mice whose name contains one of these
theme = "default"    # or a file in themes/, without .toml
open_settings = { shape = "circle" }   # or a stroke, like { stroke = "U39D" }
```

### Hints

Hints show the stroke so far as arrows, what it could still become, and the name of the gesture that fired. They fade out with the trail.

```toml
[hint]
enabled = true
next = true                  # list what the stroke could still become
position = "bottom center"   # top, center or bottom, with left, center or right
margin = 64                  # pixels from the screen edge
font = "sans-serif"          # any installed font family
size = 17                    # text size in pixels
text = "#f2f2f7"
glyph = "#9ec4ff"            # the arrows
background = "#1c1e26"       # "#rrggbbaa" for a see-through background
opacity = 0.9
padding = 14
radius = 14
```

### Themes

```toml
# ~/.config/mousewand/themes/ember.toml
color = "#ff8a3d"     # the trail
rejected = "#7f8ca3"  # the trail when nothing matched
width = 34            # pixels at the head, glow included
life_ms = 650         # how long the tail lingers
fade_ms = 400         # fade after release
glow = 0.9            # 0 is a plain line
```

For a gradient, list colors from head to tail:

```toml
color = ["#ffd36e", "#ff8a5c", "#e85d8a", "#7b4bb7"]
```

The examples include ember, laser, sakura, mint, rainbow, sunset and fire.

### Using the selected text

Commands can read the selection: `wl-paste --primary` prints it on Wayland, and `xclip -o` on X11.

```toml
[[gesture]]
stroke = "UR"
name = "Copy selection"
action = { command = "wl-paste --primary | wl-copy" }

[[gesture]]
stroke = "UL"
name = "Search selection"
action = { command = 'xdg-open "https://duckduckgo.com/?q=$(wl-paste --primary --no-newline | jq -sRr @uri)"' }
```

## Desktops

| | Gestures go to | `dispatch` runs |
|---|---|---|
| Hyprland | the window under the cursor | a dispatcher: `'hl.dsp.focus({ workspace = "e+1" })'` with a Lua config, `"workspace e+1"` before |
| KDE Plasma 6 | the window under the cursor | a KWin shortcut by name, like `"Window Maximize"` |
| GNOME | the window under the cursor | nothing |
| X11 (Xfce, Cinnamon, MATE, i3 and others) | the window under the cursor | nothing |
| sway | the focused window | a sway command, like `"fullscreen toggle"` |
| niri | the focused window | a niri action, like `"maximize-column"` |
| COSMIC, labwc | the focused window | nothing |

sway, niri, COSMIC and labwc don't tell other programs which window is under the cursor, so gestures go to the focused one. With focus-follows-mouse on, that's the same window.

On start, mousewand sets up what it can:

- Hyprland: it turns off the layer animation for its trail in the running session, and again after each config reload.
- GNOME: it installs and enables its Shell extension, which draws the trail. GNOME loads new extensions at login, so log out and back in once.
- niri: it adds `focus-follows-mouse` to `config.kdl` once, after `niri validate` accepts the change, and keeps the old file as `config.kdl.mousewand-backup`.
- X11: it starts picom for the trail if no compositing manager is running, and stops it on exit.

It tells you once about anything left to do by hand, like focus-follows-mouse in labwc or COSMIC, in the settings app or in a notification. To pick a backend yourself, set `MOUSEWAND_BACKEND` to `hyprland`, `sway`, `niri`, `kwin`, `gnome`, `x11` or `toplevel`.

## Troubleshooting

The log is in `journalctl --user -u mousewand -f`, or in the terminal if you run `mousewand` there. `RUST_LOG=mousewand=debug` adds the window class each gesture matched and the stroke it saw.

### Nothing happens

The log should say `grabbed` and your mouse's name. If it doesn't, or it asks whether `/dev/uinput` is writable, the udev rule hasn't taken effect yet: log out and back in. With several mice, check that `devices` in `config.toml` includes yours.

### A stroke does nothing

The log shows what mousewand saw, like `DUDU: no gesture`. Draw more slowly, or lower `min_segment`.

### The wrong window reacts

On sway, niri, COSMIC and labwc, gestures go to the focused window. Turn on focus-follows-mouse.

### No trail

On GNOME, log out and back in once after the first start. On X11, mousewand needs picom or another compositing manager. Anywhere else, look for `no trail:` in the log. The trail needs a working GPU driver.

### Gestures start when I meant to click or drag

A click stays a click unless you draw a stroke, and holding still for `hold_ms` makes a normal press, for middle-drag and autoscroll. If quick flicks still turn into strokes, raise `min_segment`.

### A circle does something else

A stroke with a gesture takes precedence over a shape, and a repeating stroke takes over as soon as it's drawn. Debug logging shows which stroke your circle made.

### Rings pulse around the cursor

A repeating gesture took over, and each ring is one repeat. A one-direction repeating stroke, like the example alt-tab on `9`, catches every stroke that starts in that direction. Give it a longer stroke.

### My mouse settings stopped applying

The compositor now sees one virtual mouse, "mousewand virtual mouse". Apply per-device settings, like an acceleration profile, to it.

### Keys come out wrong

`keys` uses US key positions. On another layout, write the key in the same place on a US keyboard.

### No hints

Check `enabled = true` under `[hint]`. If hints show arrows but no text, the `font` isn't installed. `fc-match "Font Name"` shows what it resolves to.

### "already a [global] gesture"

An app file uses the same input as a global gesture that shares the main trigger. Change one of them, or give `[global]` its own `trigger`.

### "there's no `kind` here", or `hold` or `rocker`

The gesture format changed. `kind = "repeat"` is now `repeat = true`, `hold` goes in the action, and `rocker` is now `click`.

### The mouse is stuck

Stopping mousewand gives it back: `systemctl --user stop mousewand`, or `pkill mousewand`.

## License

MIT or Apache-2.0, at your option.
