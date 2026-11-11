#!/usr/bin/env bash
# Restarts KWin and tiles two foot windows: "foot" left, "other" right.
export XDG_RUNTIME_DIR=/run/user/1000 WAYLAND_DISPLAY=wayland-0
echo kwin > ~/compositor
pkill -x kwin_wayland
pkill -x sway
pkill -x gnome-shell
pkill -x foot
sleep 8
(setsid foot > /dev/null 2>&1 &)
sleep 1
(setsid foot --app-id=other > /dev/null 2>&1 &)
sleep 2
ID=$(busctl --user call org.kde.KWin /Scripting org.kde.kwin.Scripting loadScript ss "$HOME/kwin-layout.js" layout | cut -d" " -f2)
busctl --user call org.kde.KWin /Scripting/Script"$ID" org.kde.kwin.Script run
sleep 1
busctl --user call org.kde.KWin /Scripting org.kde.kwin.Scripting unloadScript s layout > /dev/null
