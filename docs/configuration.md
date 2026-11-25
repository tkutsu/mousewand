# Configuring mousewand

Settings live in `~/.config/mousewand/`:

```
~/.config/mousewand/
  config.toml         settings, gestures for every app, global gestures, hints
  apps/<class>.toml   gestures for one app, by window class
  shapes/<name>.toml  shapes drawn in the settings app
  themes/<name>.toml  trail themes
```

Changes apply when you save. If a file has an error, mousewand keeps the last working config and shows the error in a notification.

The examples set up Firefox, kitty, volume on the wheel, an alt-tab and hints. Their two workspace gestures use Hyprland dispatchers.

```sh
mkdir -p ~/.config/mousewand
cp -r examples/* ~/.config/mousewand/
```

## Gestures

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

- `stroke = "DR"`: directions in the order you draw them. `U`, `D`, `L` and `R`, and diagonals as numpad digits: `7` up-left, `9` up-right, `1` down-left, `3` down-right. When nothing matches exactly, the closest stroke fires if it's within `stroke_tolerance`: a rounded corner drawn as `U9R` counts as `UR`. Repeat and hold strokes only match exactly.
- `shape = "circle"`: a drawn shape. See [Shapes](#shapes).
- `wheel = "up"`: a wheel notch while you hold the trigger, `up`, `down`, `left` or `right`. It fires on every notch.
- `click = "right"`: a `left` or `right` click while you hold the trigger. With `right` as the trigger, only `left`.

A stroke or shape can also need keys held on the keyboard, so one stroke can do two things. See [Modifiers](#modifiers).

The action is one of these:

- `keys = "ctrl+shift+t"`: a key combo for the window you drew over. Keys are positions on a US keyboard, whatever your layout.
- `command = "..."`: a shell command. It gets `$MOUSEWAND_CLASS` and `$MOUSEWAND_TITLE`, and the selected text in `$SELECTION`. See [Using the selected text](#using-the-selected-text).

- `dispatch = "..."`: a compositor command. See [Desktops](#desktops) for what each one takes.

An action can also be a list of steps, run one after another. Each step waits for the one before it to finish, a command for up to 3 seconds, and `wait` pauses for that many milliseconds:

```toml
[[gesture]]
stroke = "UR"
name = "Duplicate tab"
action = [{ keys = "ctrl+l" }, { wait = 50 }, { keys = "alt+Enter" }]
```

A stroke can keep firing while you hold the trigger. With `repeat = true` it fires when it's recognized, again for every `step` pixels (80 by default) you keep moving in its last direction, and on every wheel notch, which suits scrubbing through tabs. With `hold` in the action it does the same and holds those keys down until you let go, which makes a real alt-tab:

```toml
[[gesture]]
stroke = "9"
name = "Switch windows"
action = { keys = "Tab", hold = "alt" }
```

A repeating stroke takes over as soon as it's drawn, so no other stroke may start with it. mousewand rejects a config where one does.

## Shapes

A shape matches by how the drawing looks rather than its directions. A circle counts when it's squashed, tilted, drawn either way round, or not quite closed.

`circle`, `triangle`, `square`, `star`, `heart`, `check`, `spiral` and `infinity` are built in. To add a shape, draw it five times in the settings app. It saves the shape as `shapes/<name>.toml` and lists every shape under Shapes, where you can redraw or remove the ones you drew.

```toml
[[gesture]]
shape = "circle"
name = "Reopen tab"
action = { keys = "ctrl+shift+t" }
```

Shapes fire on release and can't repeat or hold keys. A stroke with a gesture of its own takes precedence over a shape. Among shapes, a drawing matches the one it looks most like, but only shapes some gesture uses compete: with just `circle` in use, a square is round enough to count as a circle.

## Modifiers

With `with`, a stroke or shape only fires while those keys are held on the keyboard, and the same stroke without them can do something else:

```toml
[[gesture]]
stroke = "L"
name = "Back"
action = { keys = "alt+Left" }

[[gesture]]
stroke = "L"
with = "shift"
name = "Previous tab"
action = { keys = "ctrl+shift+Tab" }
```

`with` takes `ctrl`, `shift`, `alt` and `super`, joined with `+`, and the keys must match exactly. Only strokes and shapes that fire on release can use it. Their keys are typed once you let go of the modifiers too, so Shift doesn't turn `ctrl+t` into `ctrl+shift+t`.

mousewand needs to read the keyboard to know what's held. It only asks which keys are down at the moment a gesture fires, and never reads what you type, but the access it needs would let any program you run read your typing. That's why this udev rule is separate, and only worth installing if you use `with`:

```sh
sudo cp dist/71-mousewand-keyboard.rules /etc/udev/rules.d/
sudo udevadm control --reload&& sudo udevadm trigger
```

Being in the `input` group works too.

## Turning gestures off

Gestures are off over games, and over anything else that wants the button for itself. There the trigger is an ordinary button, and so is the `[global]` trigger. `[off]` in `config.toml` lists such windows by class. `*` matches any run of characters and `?` matches one, ignoring case. `games`, on by default, covers Steam games run through Proton (`steam_app_*`), anything run through Wine (`*.exe`) and gamescope. Native Linux games have classes of their own, so list those:

```toml
[off]
games = true
match = ["blender", "org.gnome.*"]
```

The General page of the settings app edits the same list. An app file can also turn its app off:

```toml
# ~/.config/mousewand/apps/blender.toml
enabled = false
```

`mousewand toggle` pauses gestures everywhere, and running it again resumes them. Bind it to a keyboard shortcut in your desktop's settings.

## Global gestures

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

## Settings

These are the defaults:

```toml
trigger = "middle"   # middle, right, back (mouse 4), forward (mouse 5), mouse6 to mouse8
threshold = 8        # pixels to move before a press becomes a gesture
hold_ms = 300        # holding still this long makes it a normal press
cancel_ms = 0        # holding still this long mid-stroke cancels it, e.g. 700; 0 never does
min_segment = 30     # shorter strokes are ignored as noise, and so are any under a tenth of the drawing
stroke_tolerance = 30   # degrees a drawing may stray from a stroke and still fire it; 0 only exact
devices = []         # only use mice whose name contains one of these
theme = "default"    # a built-in theme, or a file in themes/ without .toml
open_settings = { shape = "circle" }   # or a stroke, like { stroke = "U39D" }
```

## Hints

Hints show the stroke so far as arrows and, in a list beside it, the name of the gesture it matches and what it could still become. A wheel or click gesture shows a mouse with its wheel or button lit, and one that fires again and again, a repeat or the wheel, counts up: "Next tab 3×". Before you draw anything, the list of what you could do includes the wheel and click gestures. Hints fade out with the trail. They're off until you turn them on, under Appearance in the settings app, which also picks a style and shows examples.

```toml
[hint]
enabled = true
next = true                  # list what the stroke could still become
position = "bottom center"   # top, center or bottom, with left, center or right
margin = 64                  # pixels from the screen edge
font = "sans-serif"          # any installed font family
size = 17                    # the stroke's size in pixels
next_size = 14               # the list's text size in pixels
text = "#f2f2f7"
next_text = "#8e8e93"        # what the stroke could still become
glyph = "#9ec4ff"            # the arrows, mice and counts
failed = "#ff5959"           # arrows once the stroke matches nothing
background = "#1c1e26"       # "#rrggbbaa" for a see-through background
opacity = 0.9
padding = 14
radius = 14
```

## Themes

Built in:

- Cool: default, ocean, aurora, mint, matrix
- Warm: ember, fire, sunset, gold
- Soft: sakura, cotton-candy, lavender
- Lines: laser, ink
- Bright: neon, rainbow
- Flags: pride, trans
- Desktop color schemes: catppuccin, dracula, gruvbox, nord, tokyo-night

To make your own, add a file to `themes/`. A file with a built-in name replaces that theme.

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

## Using the selected text

A command that mentions `$SELECTION` gets the text you last selected in it, read as the gesture fires. That's the primary selection, which selecting text sets without copying it, from whichever window you selected in. mousewand reads it with `wl-paste` on Wayland and `xclip` on X11, so one of them has to be installed.

```toml
[[gesture]]
stroke = "UR"
name = "Copy selection"
action = { command = 'printf %s "$SELECTION" | wl-copy' }

[[gesture]]
stroke = "UL"
name = "Search selection"
action = { command = 'xdg-open "https://duckduckgo.com/?q=$(printf %s "$SELECTION" | jq -sRr @uri)"' }
```

## Desktops

| | Gestures go to | `dispatch` runs |
|---|---|---|
| Hyprland | the window under the cursor | a dispatcher in Lua, like `'hl.dsp.focus({ workspace = "e+1" })'` |
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
