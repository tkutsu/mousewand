# Testing

`cargo test` runs the unit tests for recognition, the config and the gesture state machine.

The settings app has its own tests, run from its folder. The first time, `pnpm exec playwright install chromium` fetches the browser they use.

```sh
cd crates/mousewand-settings/ui
pnpm test        # the TypeScript logic, with Vitest
pnpm test:ui     # clicking through the app in Chromium, against a fake backend
pnpm test:look   # every page, light and dark, against the shots in e2e/look/
```

After a change meant to alter how a page looks, `pnpm test:look --update-snapshots` takes new shots.

## On every desktop

`tools/vm/vm.sh` runs mousewand end to end in a throwaway Arch VM on Hyprland, sway, KDE, GNOME, niri, labwc, COSMIC and X11. A fake mouse inside the guest draws over two terminal windows, and the suite checks what each gesture did: per-app gestures, keys, lists of steps, repeating and held keys, the wheel, clicks, shapes, modifiers, pausing, config reloads, click replay, `$SELECTION` and `dispatch`. It needs qemu with KVM and 8 GB of free memory.

```sh
tools/vm/vm.sh setup       # once: download the image
tools/vm/vm.sh start       # boot; the first boot installs the desktops
tools/vm/vm.sh test        # every desktop, or some: tools/vm/vm.sh test sway kwin
tools/vm/vm.sh stop
```

Each run also takes screenshots of the trail and the hint in the middle of a stroke. Subtracting the shot taken just before the stroke leaves only what mousewand drew, and that must match what it drew on Hyprland: the same place, shape and color, within a tolerance, since GNOME and X11 draw with their own code. `~/.cache/mousewand-vm/report.html` shows each desktop's drawing beside Hyprland's. The VM's Hyprland takes its look from `settings.lua`, `decoration.lua` and `animation.lua` in your `~/.config/hypr/configs/`, if they're there.

If port 2222 is taken, set `MOUSEWAND_VM_PORT` to another one.
