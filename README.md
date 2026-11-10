# wisp

Mouse gestures for Linux. Hold a mouse button, draw a gesture, and an action fires. The action depends on the window under the cursor where the gesture started.

Status: early development. Hyprland comes first; other compositors come later.

## Planned

- Direction gestures ("L", "UD", "DR"…), plus repeat, hold, wheel and rocker gestures
- Per-app actions keyed by window class: key shortcuts, shell commands, compositor dispatches
- A trigger button that still works normally: short clicks are replayed, so middle-click paste keeps working
- A GPU-drawn trail on a layer-shell overlay that exists only while you draw
- Hot-reloaded TOML config in `~/.config/wisp/`
