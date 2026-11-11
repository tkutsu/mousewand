#!/usr/bin/env bash
# Usage: gnome-setup.sh restart|window APP_ID
export XDG_RUNTIME_DIR=/run/user/1000 WAYLAND_DISPLAY=wayland-0
case "$1" in
restart)
  ext=~/.local/share/gnome-shell/extensions/wisp@tkutsu.github.io
  mkdir -p "$ext"
  cp ~/ext/* "$ext"/
  gsettings set org.gnome.shell disable-user-extensions false
  gsettings set org.gnome.shell enabled-extensions "['wisp@tkutsu.github.io']"
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
