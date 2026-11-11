# wisp

Mouse gestures for Linux. Hold a mouse button, draw a stroke, and an action fires. Which action depends on the window under the cursor where the stroke started: "left" can go back in Firefox and switch tabs in kitty.

Status: early. Works on Hyprland, KWin, GNOME, sway, niri, COSMIC, labwc and X11 desktops.

## Compositors

wisp detects the compositor on its own. To choose, set `WISP_BACKEND` to `hyprland`, `sway`, `niri`, `kwin`, `gnome`, `x11`, or `toplevel` (any compositor below that lists its windows).

| | Gestures target | Trail | `dispatch` runs |
|---|---|---|---|
| Hyprland | the window under the cursor | ✓ | a Hyprland dispatcher |
| KWin (Plasma 6) | the window under the cursor, through short KWin scripts over D-Bus | ✓ | a KWin shortcut by name, e.g. `"Window Maximize"` |
| GNOME | the window under the cursor, through wisp's Shell extension | ✓, drawn by the extension | nothing; use `keys` or `command` |
| X11 (Xfce, Cinnamon, MATE, i3, …) | the window under the cursor | ✓ with a compositing manager | nothing |
| sway | the focused window | ✓ | a sway command, e.g. `"fullscreen toggle"` |
| niri | the focused window | ✓ | a niri action, e.g. `"maximize-column"` |
| COSMIC, labwc | the focused window | ✓ | nothing |

"The focused window" means these compositors don't tell other programs which window is under the cursor. With focus-follows-mouse, the focused window is the one under the cursor anyway: sway has it on by default. Without it, gestures act on whichever window has focus, wherever you draw.

Keys and dispatchers go to the window the gesture was drawn over: wisp focuses it first. That window is fixed when the gesture starts, so it doesn't matter where the stroke ends.

### Setup

wisp sets up what it can on its own when it starts:

- **Hyprland**: adds a layer rule that turns off Hyprland's animation for the trail, so it doesn't slide in as you start drawing. The rule lives only in the running Hyprland, and wisp adds it again after every config reload.
- **GNOME**: installs its Shell extension in `~/.local/share/gnome-shell/extensions` and enables it, and updates it when wisp updates. GNOME starts new extensions only at login, so log out and back in once; until then gestures work, but only the global ones, and without the trail.
- **niri**: adds `focus-follows-mouse` to the `input` section of `config.kdl`, after `niri validate` accepts the change, and keeps the old file as `config.kdl.wisp-backup`. It does this once: remove the setting and wisp leaves it out.
- **X11**: the trail needs a compositing manager. If none is running, wisp starts picom (without any picom config, so no shadows or fades) and stops it when it stops.

What it can't do, such as turning on focus-follows-mouse in labwc or COSMIC, or logging you out, it tells you: the settings app opens on a Setup tab, or, without the app, a desktop notification appears. Each note shows once, and again only when there's a new one, such as after switching to another compositor. The notes for the current desktop stay in the app's Setup tab and in `~/.local/state/wisp/notes`.

GNOME has no layer-shell, so its trail is drawn inside GNOME Shell by the extension, with Cairo instead of the GPU shader. Themes apply the same way; the glow is a close approximation.

## How it behaves

- **Click**: a quick press of the trigger is replayed as a normal click, so middle-click paste still works.
- **Hold**: holding the trigger still for 300 ms makes it a normal press, so middle-drag and autoscroll still work.
- **Gesture**: moving more than a few pixels while holding draws a stroke. Releasing fires the matching action, and the trigger click never reaches the app.
- **Wheel and rocker**: while the trigger is down, wheel notches and other button presses belong to wisp. They fire their own gestures and never reach the app, and the trigger click isn't replayed.

The trail turns red and fades when a stroke matches nothing.

Strokes are sequences of directions: `U D L R` for up, down, left and right, and numpad digits for diagonals: `7` ↖, `9` ↗, `1` ↙, `3` ↘. `"DR"` is down then right; `"3"` is one down-right line.

## Install

```fish
cargo install --path crates/wisp
sudo cp dist/70-wisp.rules /etc/udev/rules.d/
sudo cp dist/wisp-uinput.conf /etc/modules-load.d/
sudo modprobe uinput; and sudo udevadm control --reload; and sudo udevadm trigger
cp dist/wisp.service ~/.config/systemd/user/
systemctl --user enable --now wisp
```

The udev rule lets your user read mice and create the virtual device. Being in the `input` group works too.

## Config

```
~/.config/wisp/
  config.toml        # settings and global gestures
  apps/<class>.toml  # gestures for one app, keyed by window class
  themes/<name>.toml # trail looks, picked with theme = "<name>"
```

```toml
# apps/firefox.toml
match = ["firefox", "librewolf"]

[[gesture]]
stroke = "L"
action = { keys = "alt+Left" }
```

Actions:

- `keys`: a key combo like `"ctrl+shift+t"`, typed into the window the stroke started on. Keys are physical (US QWERTY positions), whatever your layout.
- `command`: a shell command. It gets `$WISP_CLASS` and `$WISP_TITLE`.
- `dispatch`: a Hyprland dispatcher, passed as is. Lua-config Hyprland takes `'hl.dsp.focus({ workspace = "e+1" })'`; older versions take `"workspace e+1"`.

Each gesture has exactly one input:

- `stroke = "DR"`: a drawn stroke.
- `wheel = "up"`: one wheel notch (`up`, `down`, `left`, `right`) while the trigger is held. Fires on every notch.
- `rocker = "right"`: a `left` or `right` click while the trigger is held.

Strokes can also keep firing while you hold the trigger, with `kind`:

- `kind = "repeat"`: fires as soon as the stroke is recognized, then again every `step` pixels (80 by default) you keep moving in its last direction, and on every wheel notch. Good for scrubbing through tabs.
- `kind = "hold"`, with `hold = "alt"`: like repeat, with those keys held down from the match until you release the trigger. `stroke = "9"`, `hold = "alt"`, `action = { keys = "Tab" }` is a real alt-tab switcher.

A repeat or hold stroke fires the moment it's recognized, so no other stroke may start with it; wisp rejects such a config with an error. Held keys are released however the gesture ends: trigger release, the mouse unplugged, or wisp stopped.

App gestures override global ones with the same input. Saving a file reloads it; a broken file is logged and the last good config stays in use. See [`examples/`](examples) for a fuller setup. To find a window's class: `hyprctl clients`.

## Settings app

Draw a W (↘↗↘↗, written `3939`) over any window to open `wisp-settings` on that window's app: its gestures, presets for common apps, and a Record button that captures a shortcut as you press it. The "Everywhere" tab holds the trigger, theme and global gestures. A gesture of your own on `3939` replaces this one.

The config files stay the source of truth: the app edits only the settings and gesture lists, keeps your comments, and refuses to save anything the daemon would reject. It's a Tauri app, built separately:

```fish
pnpm -C crates/wisp-settings/ui install
pnpm -C crates/wisp-settings/ui build
cargo install --path crates/wisp-settings
```

`wisp-settings --class firefox` opens it on an app by hand.

## Themes

```toml
# themes/ember.toml
color = "#ff8a3d"     # the trail
rejected = "#7f8ca3"  # the trail when nothing matched
width = 34            # px at the head, glow included
life_ms = 650         # how long the tail lingers
fade_ms = 400         # fade after release
glow = 0.9            # 0 is a plain line
```

Repeat and hold gestures pulse a ring at the cursor on every step.

## How it works

wisp grabs your mice through evdev and forwards everything through one virtual uinput device, except the trigger button, which it holds back until it knows whether the press is a click, a hold or a gesture. Because the compositor sees the virtual device, per-mouse libinput settings (like a custom accel profile for one mouse) apply to "wisp virtual mouse" instead.

| Crate | Does |
|---|---|
| `wisp-core` | Config, stroke recognition, the trigger state machine, and the per-gesture session that decides what fires. Pure and unit-tested. |
| `wisp-input` | evdev grab, uinput forwarding, click replay, key injection. |
| `wisp-backend` | Window under the cursor, focus and dispatch, per compositor. |
| `wisp-overlay` | The trail: a layer-shell surface drawn with wgpu. |
| `wisp` | The daemon. |

The trail's surface exists only while you draw: plain clicks never create one, so it can't get in the way of fullscreen games or video. The GPU device starts once with the daemon so each gesture only pays for a surface. Its layer namespace is `wisp`.

`cargo run -p wisp-input --example probe` tests the input path against a fake mouse without touching your desktop.

`tools/vm/vm.sh` tests the other compositors in a QEMU VM, end to end: a fake mouse inside the guest, wisp's grab, and the compositor on top. `vm.sh setup`, `vm.sh start`, then `vm.sh test sway` (or `kwin`, `gnome`, `niri`, `labwc`, `cosmic`, `x11`).

## License

MIT or Apache-2.0, at your option.
