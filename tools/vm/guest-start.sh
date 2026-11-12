#!/usr/bin/env bash
# Usage: start.sh COMPOSITOR — starts COMPOSITOR with the suite's two windows and
# prints its display: a Wayland one such as wayland-2, or :0 on X11.
#
# Every compositor ends up the same: a 1600×900 output with flat pointer
# acceleration, so the fake mouse's moves are pixels; "foot" titled "left" on
# the left half and "other" titled "right" on the right, both logging the keys
# typed into them (keylog.sh). niri, labwc and COSMIC need a GPU on the bare
# display, so they run fullscreen inside sway. Hyprland can't run nested in
# sway's software renderer, but runs on the bare display.
set -u
export XDG_RUNTIME_DIR=/run/user/1000

# windows OPEN: opens both windows with OPEN, a command that takes the class,
# the title and then the program.
windows() {
  $1 foot left bash ~/keylog.sh left
  sleep 1.5
  $1 other right bash ~/keylog.sh right
  sleep 2
}

foot_on() { (WAYLAND_DISPLAY=$display setsid foot --app-id="$1" --title="$2" "${@:3}" > /dev/null 2>&1 &); }

sway_up() {
  bash ~/switch.sh sway > /dev/null
  export SWAYSOCK=$(ls -t /run/user/1000/sway-ipc.*.sock | head -1)
  swaymsg -q 'bar mode invisible; input type:pointer accel_profile flat'
}

# The newest Wayland socket is the compositor that started last.
newest() { ls -t /run/user/1000 | grep -E "^wayland-[0-9]+$" | head -1; }

nested() {
  sway_up
  swaymsg -q "exec $1 > ~/nested-$C.log 2>&1"
  sleep 6
  swaymsg -q fullscreen enable
  sleep 1
  display=$(newest)
  windows foot_on
}

C=${1:?hyprland, sway, kwin, gnome, niri, labwc, cosmic or x11}
case "$C" in
  hyprland)
    bash ~/switch.sh hyprland > /dev/null
    display=$(newest)
    windows foot_on
    ;;
  # mousewand turns on focus-follows-mouse itself; start from niri's default.
  niri)
    cp ~/.config/niri/config.kdl.mousewand-backup ~/.config/niri/config.kdl 2> /dev/null &&
      rm ~/.config/niri/config.kdl.mousewand-backup
    nested niri
    ;;
  labwc) nested labwc ;;
  cosmic) nested cosmic-comp ;;
  sway)
    sway_up
    display=$(newest)
    windows foot_on
    ;;
  kwin)
    bash ~/switch.sh kwin > /dev/null
    display=wayland-0
    windows foot_on
    ID=$(busctl --user call org.kde.KWin /Scripting org.kde.kwin.Scripting loadScript ss "$HOME/kwin-layout.js" layout | cut -d" " -f2)
    busctl --user call org.kde.KWin /Scripting/Script"$ID" org.kde.kwin.Script run
    sleep 1
    busctl --user call org.kde.KWin /Scripting org.kde.kwin.Scripting unloadScript s layout > /dev/null
    ;;
  # mousewand installs its extension, and GNOME loads extensions at login: a
  # Shell restart stands in for logging in again. mousewand-tile@test tiles the windows.
  gnome)
    shell_up() {
      bash ~/switch.sh gnome > /dev/null
      for _ in $(seq 30); do busctl --user status org.gnome.Shell > /dev/null 2>&1 && break; sleep 1; done
      sleep 3
    }
    gsettings set org.gnome.shell disable-user-extensions false
    gsettings set org.gnome.desktop.peripherals.mouse accel-profile flat
    shell_up
    (MOUSEWAND_CONFIG_DIR=~/cfg timeout 4 ~/bin/mousewand > ~/install.log 2>&1)
    list=$(gsettings get org.gnome.shell enabled-extensions)
    case "$list" in
      *mousewand-tile@test*) ;;
      "@as []") gsettings set org.gnome.shell enabled-extensions "['mousewand-tile@test']" ;;
      *) gsettings set org.gnome.shell enabled-extensions "${list%]}, 'mousewand-tile@test']" ;;
    esac
    shell_up
    busctl --user set-property org.gnome.Shell /org/gnome/Shell org.gnome.Shell OverviewActive b false
    display=wayland-0
    windows foot_on
    ;;
  # xfwm4 without its compositor, so mousewand starts picom.
  x11)
    bash ~/switch.sh x11 > /dev/null
    sleep 3
    display=:0
    xterm_on() {
      local geometry=98x58+0+0
      [ "$2" = right ] && geometry=98x58+800+0
      # foot's colors, so the screenshots' background matches the others'.
      (DISPLAY=:0 setsid xterm -class "${1^}" -title "$2" -geometry $geometry -bg "#242424" -fg "#dcdccc" \
        -e "${@:3}" > /dev/null 2>&1 &)
    }
    windows xterm_on
    ;;
  *) echo "unknown compositor $C" >&2; exit 1 ;;
esac
echo "$display"
