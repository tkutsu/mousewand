# wisp

Mouse gestures for Linux. Hold a mouse button, draw a stroke, and an action fires. Which action depends on the window under the cursor where the stroke started: "left" can go back in Firefox and switch tabs in kitty.

Status: early. Works on Hyprland; other compositors come later.

## How it behaves

- **Click**: a quick press of the trigger is replayed as a normal click, so middle-click paste still works.
- **Hold**: holding the trigger still for 300 ms makes it a normal press, so middle-drag and autoscroll still work.
- **Gesture**: moving more than a few pixels while holding draws a stroke. Releasing fires the matching action, and the trigger click never reaches the app.

Strokes are sequences of directions: `U D L R` for up, down, left and right, and numpad digits for diagonals: `7` ↖, `9` ↗, `1` ↙, `3` ↘. `"DR"` is down then right; `"3"` is one down-right line.

## Install

```fish
cargo install --path crates/wisp
sudo cp dist/70-wisp.rules /etc/udev/rules.d/
sudo udevadm control --reload; and sudo udevadm trigger
cp dist/wisp.service ~/.config/systemd/user/
systemctl --user enable --now wisp
```

The udev rule lets your user read mice and create the virtual device. Being in the `input` group works too.

## Config

```
~/.config/wisp/
  config.toml        # settings and global gestures
  apps/<class>.toml  # gestures for one app, keyed by window class
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

App gestures override global ones with the same stroke. Saving a file reloads it; a broken file is logged and the last good config stays in use. See [`examples/`](examples) for a fuller setup. To find a window's class: `hyprctl clients`.

## How it works

wisp grabs your mice through evdev and forwards everything through one virtual uinput device, except the trigger button, which it holds back until it knows whether the press is a click, a hold or a gesture. Because the compositor sees the virtual device, per-mouse libinput settings (like a custom accel profile for one mouse) apply to "wisp virtual mouse" instead.

| Crate | Does |
|---|---|
| `wisp-core` | Config, stroke recognition, the trigger state machine. Pure and unit-tested. |
| `wisp-input` | evdev grab, uinput forwarding, click replay, key injection. |
| `wisp-backend` | Window under the cursor, focus and dispatch, per compositor. |
| `wisp-overlay` | The trail: a layer-shell surface drawn with wgpu. |
| `wisp` | The daemon. |

The trail's surface exists only while you draw: plain clicks never create one, so it can't get in the way of fullscreen games or video. The GPU device starts once with the daemon so each gesture only pays for a surface. Its layer namespace is `wisp`; turn off Hyprland's layer animation for it with a `noanim` layer rule so the trail appears instantly.

`cargo run -p wisp-input --example probe` tests the input path against a fake mouse without touching your desktop.

## License

MIT or Apache-2.0, at your option.
