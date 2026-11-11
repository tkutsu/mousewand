#!/usr/bin/env bash
# Usage: gnome-setup.sh restart|install|window APP_ID
export XDG_RUNTIME_DIR=/run/user/1000 WAYLAND_DISPLAY=wayland-0
case "$1" in
# wisp installs and enables its extension; GNOME loads it at the next login.
install)
  for _ in $(seq 30); do busctl --user status org.gnome.Shell > /dev/null 2>&1 && break; sleep 1; done
  (WISP_CONFIG_DIR=~/cfg timeout 4 ~/bin/wisp > ~/install.log 2>&1)
  grep -E "installed|notes" ~/install.log | cut -c25-
  gsettings get org.gnome.shell enabled-extensions
  ;;
restart)
  gsettings set org.gnome.shell disable-user-extensions false
  echo gnome > ~/compositor
  pkill -x kwin_wayland
  pkill -x sway
  pkill -x gnome-shell
  sleep 15
  busctl --user set-property org.gnome.Shell /org/gnome/Shell org.gnome.Shell OverviewActive b false
  gnome-extensions info wisp@tkutsu.github.io | grep -i state
  ;;
window)
  pkill -x foot
  sleep 1
  (setsid foot --maximized --app-id="$2" > /dev/null 2>&1 &)
  sleep 3
  busctl --user set-property org.gnome.Shell /org/gnome/Shell org.gnome.Shell OverviewActive b false
  ;;
esac
