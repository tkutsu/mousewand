# Troubleshooting

The log is in `journalctl --user -u mousewand -f`, or in the terminal if you run `mousewand` there. `RUST_LOG=mousewand=debug` adds the window class each gesture matched and the stroke it saw.

## Nothing happens

The log should say `grabbed` and your mouse's name. If it doesn't, or it asks whether `/dev/uinput` is writable, the udev rule hasn't taken effect yet: log out and back in. With several mice, check that `devices` in `config.toml` includes yours.

## A stroke does nothing

The log shows what mousewand saw, like `DUDU: no gesture`, or `U9R: close to UR` when it fired the closest stroke. Draw more slowly, or lower `min_segment`.

## The wrong window reacts

On sway, niri, COSMIC and labwc, gestures go to the focused window. Turn on focus-follows-mouse.

## No trail

On GNOME, log out and back in once after the first start. On X11, mousewand needs picom or another compositing manager. Anywhere else, look for `no trail:` in the log. The trail needs a working GPU driver.

## Gestures start when I meant to click or drag

A click stays a click unless you draw a stroke, and holding still for `hold_ms` makes a normal press, for middle-drag and autoscroll. If quick flicks still turn into strokes, raise `min_segment`.

## A gesture with Shift or Ctrl does nothing

The log says `can't read the keyboard` once if mousewand can't see which keys are held. Install the rule in [Modifiers](configuration.md#modifiers), then log out and back in. A gesture `with = "shift"` doesn't fire while Ctrl is held too.

## A stroke I gave up on fired anyway

With `cancel_ms` set, holding still that long cancels a stroke, and the hint says "Cancelled". A repeating gesture that has already taken over doesn't cancel.

## A circle does something else

A stroke with a gesture takes precedence over a shape, and a repeating stroke takes over as soon as it's drawn. Debug logging shows which stroke your circle made.

## Rings pulse around the cursor

A repeating gesture took over, and each ring is one repeat. A one-direction repeating stroke, like the example alt-tab on `9`, catches every stroke that starts in that direction. Give it a longer stroke.

## My mouse settings stopped applying

The compositor now sees one virtual mouse, "mousewand virtual mouse". Apply per-device settings, like an acceleration profile, to it.

## Keys come out wrong

`keys` uses US key positions. On another layout, write the key in the same place on a US keyboard.

## No hints

Hints are off by default. Turn them on under Appearance in the settings app, or set `enabled = true` under `[hint]`. If hints show arrows but no text, the `font` isn't installed. `fc-match "Font Name"` shows what it resolves to.

## "already a [global] gesture"

An app file uses the same input as a global gesture that shares the main trigger. Change one of them, or give `[global]` its own `trigger`.

## "there's no `kind` here", or `hold` or `rocker`

The gesture format changed. `kind = "repeat"` is now `repeat = true`, `hold` goes in the action, and `rocker` is now `click`.

## The mouse is stuck

Stopping mousewand gives it back: `systemctl --user stop mousewand`, or `pkill mousewand`.
