#!/usr/bin/env bash
# Usage: switch.sh COMPOSITOR — stops whatever runs on tty1 and starts it.
echo "$1" > ~/compositor
pkill -x foot
for c in sway kwin_wayland gnome-shell niri cosmic-comp wayfire labwc river; do
  pkill -x "$c"
done
sleep 8
ls /run/user/1000 | grep -E "^wayland-[0-9]+$" | tail -1
