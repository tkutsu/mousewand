#!/usr/bin/env bash
# Usage: nested.sh COMPOSITOR — runs COMPOSITOR nested in sway with two foot
# windows ("foot", then "other"), and prints its Wayland display.
# Compositors that need a GPU refuse llvmpipe on the VM's bare display,
# but run fine as a window inside sway.
export XDG_RUNTIME_DIR=/run/user/1000
bash ~/switch.sh sway > /dev/null
S=$(ls -t /run/user/1000/sway-ipc.*.sock | head -1)
case "$1" in
  # wisp turns on focus-follows-mouse itself; start from niri's default.
  niri)
    cp ~/.config/niri/config.kdl.wisp-backup ~/.config/niri/config.kdl 2> /dev/null &&
      rm ~/.config/niri/config.kdl.wisp-backup
    cmd=niri
    ;;
  cosmic) cmd=cosmic-comp ;;
  *) cmd=$1 ;;
esac
SWAYSOCK=$S swaymsg -q "exec $cmd > ~/nested-$1.log 2>&1"
sleep 6
# The nested compositor's socket is the newest one.
display=$(ls -t /run/user/1000 | grep -E "^wayland-[0-9]+$" | head -1)
(WAYLAND_DISPLAY=$display setsid foot > /dev/null 2>&1 &)
sleep 1.5
(WAYLAND_DISPLAY=$display setsid foot --app-id=other > /dev/null 2>&1 &)
sleep 2
echo "$display"
