# wisp

Mouse gestures for Linux. Hold a mouse button, draw a stroke, and an action fires. Which action depends on the window under the cursor where the stroke started: "left" can go back in Firefox and switch tabs in kitty.

Status: early. Works on Hyprland, sway, KWin (Plasma 6) and GNOME.

## Compositors

wisp detects the compositor on its own. Set `WISP_BACKEND` to `hyprland`, `sway`, `kwin` or `gnome` to choose.

| | Window under the cursor | Trail | `dispatch` runs |
|---|---|---|---|
| Hyprland | ✓ | ✓ | a Hyprland dispatcher |
| sway | the focused window; with sway's default `focus_follows_mouse` that's the one under the cursor | ✓ | a sway command, e.g. `"fullscreen toggle"` |
| KWin | ✓, through short KWin scripts over D-Bus | ✓ | a KWin shortcut by name, e.g. `"Window Maximize"` |
| GNOME | ✓, through wisp's Shell extension | ✓, drawn by the extension | nothing; use `keys` or `command` |

Keys and dispatchers go to the window the gesture was drawn over: wisp focuses it first.

GNOME has no layer-shell, so its trail is drawn inside GNOME Shell by the extension, with Cairo instead of the GPU shader. Themes apply the same way; the glow is a close approximation.

GNOME needs the extension in `dist/gnome-extension`:

```fish
cp -r dist/gnome-extension/wisp@tkutsu.github.io ~/.local/share/gnome-shell/extensions/
# log out and back in, then:
gnome-extensions enable wisp@tkutsu.github.io
```

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

Draw ↑↓↑ (`UDU`) over any window to open `wisp-settings` on that window's app: its gestures, presets for common apps, and a Record button that captures a shortcut as you press it. The "Everywhere" tab holds the trigger, theme and global gestures. A gesture of your own on `UDU` replaces this one.

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

The trail's surface exists only while you draw: plain clicks never create one, so it can't get in the way of fullscreen games or video. The GPU device starts once with the daemon so each gesture only pays for a surface. Its layer namespace is `wisp`; turn off Hyprland's layer animation for it with a `noanim` layer rule so the trail appears instantly.

`cargo run -p wisp-input --example probe` tests the input path against a fake mouse without touching your desktop.

`tools/vm/vm.sh` tests the other compositors in a QEMU VM, end to end: a fake mouse inside the guest, wisp's grab, and sway, KWin or GNOME on top. `vm.sh setup`, `vm.sh start`, then `vm.sh test sway` (or `kwin`, `gnome`).

## License

MIT or Apache-2.0, at your option.
