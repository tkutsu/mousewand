<p align="center"><img src="docs/logo.svg" width="96" alt=""></p>

<h1 align="center">mousewand</h1>

<p align="center">Mouse gestures for every Linux desktop.<br>Hold a button, draw a stroke, and the window under it does what you meant.</p>

<p align="center"><img src="docs/demo.gif" width="800" alt="Drawing a stroke to the left in Firefox goes back a page, and a stroke to the right goes forward again. Each leaves a short orange trail, and a hint at the bottom names the gesture and what the stroke could still become."></p>

## Why mousewand

- **Works on nearly every desktop.** Hyprland, KDE Plasma, GNOME, sway, niri, COSMIC, labwc, and X11 desktops like Xfce, Cinnamon, MATE and i3. It finds your compositor on its own.
- **Different gestures for each app.** The same stroke can go back in Firefox and switch tabs in your terminal. The window under the cursor decides.
- **Stays out of your way.** A quick middle click is still a middle click, so pasting works. Holding the button still is still a drag or an autoscroll.
- **You see what you draw.** A smooth, glowing trail follows the stroke, and turns red when nothing matches. Turn on hints to see the stroke so far, what it could still become, and the name of what fired.
- **More than strokes.** Spin the wheel or click the other buttons while holding the trigger. Strokes can repeat as you keep moving, for scrubbing through tabs, or hold keys down, for a real alt-tab. Global gestures can live on a button of their own.
- **Sets itself up.** It installs what your desktop needs, like GNOME's extension, and tells you once about anything left for you to do.
- **Plain text config, applied on save.** Or draw a W to open the settings app.

## Install

**1. Build it.** You need a recent [Rust](https://rustup.rs) toolchain.

```fish
git clone https://github.com/tkutsu/mousewand
cd mousewand
cargo install --path crates/mousewand
```

**2. Let it read your mouse.** mousewand reads mice directly and sends everything on through a virtual mouse. This udev rule allows that for the logged-in user, without root:

```fish
sudo cp dist/70-mousewand.rules /etc/udev/rules.d/
sudo cp dist/mousewand-uinput.conf /etc/modules-load.d/
sudo modprobe uinput; and sudo udevadm control --reload; and sudo udevadm trigger
```

Log out and back in once. Being in the `input` group works too.

**3. Try it.** Run `mousewand` in a terminal, hold the middle button and draw. Out of the box, a W (↘↗↘↗) opens the settings app; add your own gestures below. Ctrl+C gives the mouse back.

**4. Start it with your desktop.**

```fish
mkdir -p ~/.config/systemd/user
cp dist/mousewand.service ~/.config/systemd/user/
systemctl --user enable --now mousewand
```

The settings app is optional and built separately; see [Settings app](#settings-app).

## Your first gestures

Copy the examples, then edit them. Saving a file applies it at once.

```fish
mkdir -p ~/.config/mousewand
cp -r examples/* ~/.config/mousewand/
```

They give Firefox back, forward, new tab, close tab and reopen tab, kitty tab switching, volume on the wheel, an alt-tab, and hints. The two workspace gestures in `config.toml` are for Hyprland; remove them elsewhere.

A gesture is a stroke and an action:

```toml
# ~/.config/mousewand/apps/firefox.toml
match = ["firefox", "librewolf"]

[[gesture]]
stroke = "L"                   # draw left…
name = "Back"                  # …which hints call "Back"…
action = { keys = "alt+Left" } # …to go back
```

Strokes are directions in order: `U D L R`, plus diagonals written as numpad digits, `7` ↖ `9` ↗ `1` ↙ `3` ↘. So `"DR"` is down, then right.

## Desktops

| | Gestures act on | Trail | `dispatch` runs |
|---|---|---|---|
| Hyprland | the window under the cursor | ✓ | a Hyprland dispatcher |
| KDE Plasma 6 | the window under the cursor | ✓ | a KWin shortcut by name, e.g. `"Window Maximize"` |
| GNOME | the window under the cursor | ✓ | nothing; use `keys` or `command` |
| X11 (Xfce, Cinnamon, MATE, i3, …) | the window under the cursor | ✓ | nothing |
| sway | the focused window | ✓ | a sway command, e.g. `"fullscreen toggle"` |
| niri | the focused window | ✓ | a niri action, e.g. `"maximize-column"` |
| COSMIC, labwc | the focused window | ✓ | nothing |

"The focused window" means the compositor doesn't tell other programs which window is under the cursor. With focus-follows-mouse on, that's the same window; sway has it on by default, and mousewand turns it on in niri.

On start, mousewand sets up what it can:

- **Hyprland**: turns off the layer animation for its trail, so the trail doesn't slide in. The rule lives only in the running Hyprland, and mousewand adds it again after a config reload.
- **GNOME**: installs and enables its Shell extension, and updates it with mousewand. GNOME starts new extensions only at login, so log out and back in once.
- **niri**: adds `focus-follows-mouse` to `config.kdl`, once, after `niri validate` accepts it, keeping the old file as `config.kdl.mousewand-backup`.
- **X11**: starts picom for the trail if no compositing manager is running, and stops it on exit.

Anything it can't do for you, like turning on focus-follows-mouse in labwc or COSMIC, it tells you once: in the settings app, or as a notification. To choose a backend by hand, set `MOUSEWAND_BACKEND` to `hyprland`, `sway`, `niri`, `kwin`, `gnome`, `x11` or `toplevel`.

## Configuration

```
~/.config/mousewand/
  config.toml         settings, gestures for every app, global gestures, hints
  apps/<class>.toml   gestures for one app, by window class
  themes/<name>.toml  trail looks, picked with theme = "<name>"
```

An app's gestures replace the ones in `config.toml` with the same input. A broken file is reported in the log, and the last good config stays in use.

### Settings

```toml
trigger = "middle"   # middle, side, extra (mouse 5), forward, back or task
threshold = 8        # movement, in px, before a press becomes a gesture
hold_ms = 300        # holding still this long makes it a normal press
min_segment = 30     # shorter strokes are ignored as noise
devices = []         # only use mice whose name contains one of these
theme = "default"    # or a file in themes/, without .toml
```

### Actions

- `keys`: a key combo like `"ctrl+shift+t"`, typed into the window you drew on. Keys are physical positions on a US layout, whatever your layout is.
- `command`: a shell command. It gets `$MOUSEWAND_CLASS` and `$MOUSEWAND_TITLE`.
- `dispatch`: a compositor command, for the desktops in the table above. Hyprland with a Lua config takes `'hl.dsp.focus({ workspace = "e+1" })'`; older versions take `"workspace e+1"`.

### Inputs

Each gesture has one:

- `stroke = "DR"`: a drawn stroke.
- `wheel = "up"`: a wheel notch (`up`, `down`, `left`, `right`) while holding the trigger. Fires on every notch.
- `rocker = "right"`: a `left` or `right` click while holding the trigger.

A stroke can keep firing while you hold the trigger:

- `kind = "repeat"` fires when the stroke is recognized, then again every `step` px (80 by default) you keep moving its way, and on every wheel notch. Good for scrubbing through tabs.
- `kind = "hold"` with `hold = "alt"` does the same with those keys held down until you let go. `stroke = "9"`, `kind = "hold"`, `hold = "alt"`, `action = { keys = "Tab" }` is a real alt-tab.

A repeating stroke fires as soon as it's recognized, so no other stroke may start with it; mousewand rejects such a config with an error.

Any gesture can have a `name`, which hints show. Without one, they show its keys or command.

### Global gestures

Gestures under `[global]` are the same in every app, and no app file can replace them; a config that tries is rejected. Give them a `trigger` of their own, and that button runs them and nothing else, while the main trigger keeps the app gestures:

```toml
[global]
trigger = "side"     # optional; without it they share the main trigger

[[global.gesture]]
stroke = "RU"
name = "Next workspace"
action = { dispatch = 'hl.dsp.focus({ workspace = "e+1" })' }
```

On their own button, `keys` go to whichever window has focus, not the one under the cursor.

### Hints

A small panel that shows the stroke so far as arrows, with the gesture it matches, a list of what it could still become, and the name of what fired. It fades out with the trail.

```toml
[hint]
enabled = true
next = true                  # list what the stroke could still become
position = "bottom center"   # top, center or bottom, with left, center or right
margin = 64                  # from the screen edge, px
font = "sans-serif"          # any installed font family, e.g. "Inter"
size = 17                    # text size, px; arrows match it
text = "#f2f2f7"
glyph = "#9ec4ff"            # the arrows
background = "#1c1e26"       # "#rrggbbaa" for a see-through background
opacity = 0.9                # of the whole hint
padding = 14
radius = 14
```

### Themes

```toml
# ~/.config/mousewand/themes/ember.toml
color = "#ff8a3d"     # the trail
rejected = "#7f8ca3"  # the trail when nothing matched
width = 34            # px at the head, glow included
life_ms = 650         # how long the tail lingers
fade_ms = 400         # fade after release
glow = 0.9            # 0 is a plain line
```

## Settings app

Draw a W over any window to open the settings app on that app's gestures. It has presets for common apps, a Record button that captures a shortcut as you press it, and an Everywhere tab for the trigger, theme and global gestures. It edits the same files, keeps your comments, and won't save anything mousewand would reject.

It's a Tauri app, so it needs WebKitGTK (`webkit2gtk-4.1`), Node and pnpm to build:

```fish
pnpm -C crates/mousewand-settings/ui install
pnpm -C crates/mousewand-settings/ui build
cargo install --path crates/mousewand-settings
```

`mousewand-settings --class firefox` opens it on an app by hand.

## Troubleshooting

Most answers are in the log: `journalctl --user -u mousewand -f`, or run `mousewand` in a terminal. For more detail, start it with `RUST_LOG=mousewand=debug`.

**Nothing happens at all.** Check that it's running: `systemctl --user status mousewand`. The log should say `grabbed` and your mouse's name. If it doesn't, or it asks whether `/dev/uinput` is writable, step 2 of [Install](#install) hasn't taken effect yet: log out and back in. With several mice, `devices` in `config.toml` may be leaving yours out.

**It doesn't start with my desktop.** The service starts with `graphical-session.target`, which some setups never reach. Start `mousewand` from your compositor's autostart instead, e.g. `exec-once` in Hyprland.

**A stroke does nothing.** The log shows what mousewand saw, e.g. `DUDU: no gesture`. Draw more slowly or more clearly, or lower `min_segment`. With debug logging, it also shows the window class it matched; that's the name for `apps/`.

**The wrong window reacts.** On sway, niri, COSMIC and labwc, gestures go to the focused window. Turn on focus-follows-mouse; see [Desktops](#desktops).

**No trail.** On GNOME, log out and back in once after the first start. On X11, mousewand needs picom or another compositing manager. Anywhere else, look for `no trail:` in the log; the trail needs a working GPU driver.

**Gestures start when I meant to click or drag.** A quick click passes through as a click, and holding still for `hold_ms` makes it a normal press, for middle-drag and autoscroll. If a slight wobble starts a gesture anyway, raise `threshold`.

**My mouse settings stopped applying.** Your compositor now sees one virtual mouse, "mousewand virtual mouse". Apply per-device settings, like a custom acceleration profile, to it instead.

**Keys come out wrong.** `keys` uses US key positions. On another layout, write the key that sits in the same place on a US keyboard.

**No hints.** Check `enabled = true` under `[hint]` in `config.toml`. If they show arrows but no text, the `font` isn't installed: `fc-match "Your Font"` shows what it resolves to.

**"already a [global] gesture".** An app file uses the same stroke, wheel or rocker as a global gesture that shares the main trigger. Change one of them, or give `[global]` a `trigger` of its own.

**Drawing a W shows "command not found".** The settings app isn't installed; see [Settings app](#settings-app).

**The mouse is stuck.** Stopping mousewand always gives it back: `systemctl --user stop mousewand`, or `pkill mousewand` from a keyboard.

## How it works

mousewand grabs your mice through evdev and passes everything on through one virtual uinput device, except the trigger button. It holds that back until it knows whether the press is a click, a hold or a gesture. The trail is a layer-shell surface drawn with wgpu, and it exists only while you draw, so it can't get in the way of fullscreen games or video.

| Crate | Does |
|---|---|
| `mousewand-core` | Config, stroke recognition, the trigger state machine and gesture sessions. Pure and unit-tested. |
| `mousewand-input` | evdev grab, uinput forwarding, click replay, key injection. |
| `mousewand-backend` | The window under the cursor, focus and dispatch, per compositor. |
| `mousewand-overlay` | The trail. |
| `mousewand` | The daemon. |
| `mousewand-settings` | The settings app. |

## Development

`cargo test` runs the unit tests. `cargo run -p mousewand-input --example probe` tests the input path against a fake mouse without touching your desktop.

`tools/vm/vm.sh` tests every compositor end to end in a QEMU VM: a fake mouse inside the guest, mousewand's grab, and the compositor on top. Run `vm.sh setup` and `vm.sh start`, then `vm.sh test sway` (or `kwin`, `gnome`, `niri`, `labwc`, `cosmic`, `x11`).

## License

MIT or Apache-2.0, at your option.
