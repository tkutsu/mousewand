#!/usr/bin/env bash
# Runs on tty1 after autologin: starts the compositor named in ~/compositor,
# again and again, so tests can switch by writing the file and killing it.
# Compositors use the user's session bus, so SSH sessions share it.
export XDG_SESSION_TYPE=wayland
export WLR_RENDERER=pixman
export WLR_NO_HARDWARE_CURSORS=1
while true; do
  c=$(cat ~/compositor 2> /dev/null || echo none)
  case "$c" in
    sway) sway > ~/session-sway.log 2>&1 ;;
    kwin) kwin_wayland --drm --no-lockscreen > ~/session-kwin.log 2>&1 ;;
    gnome) gnome-shell --wayland --display-server > ~/session-gnome.log 2>&1 ;;
    niri) niri > ~/session-niri.log 2>&1 ;;
    cosmic) cosmic-comp > ~/session-cosmic.log 2>&1 ;;
    wayfire) wayfire -c ~/wayfire.ini > ~/session-wayfire.log 2>&1 ;;
    labwc) labwc > ~/session-labwc.log 2>&1 ;;
    river) river > ~/session-river.log 2>&1 ;;
    x11)
      printf '#!/bin/sh\nexec xfwm4 --compositor=off\n' > ~/.xinitrc-wisp
      startx ~/.xinitrc-wisp -- :0 vt1 -keeptty > ~/session-x11.log 2>&1
      ;;
    *) sleep 2 ;;
  esac
  sleep 1
done
