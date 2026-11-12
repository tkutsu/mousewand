#!/usr/bin/env bash
# Usage: suite.sh COMPOSITOR — mousewand's features end to end on one compositor,
# in the guest. Prints a TAP line per scenario and exits 1 if any failed.
#
# One mousewand runs throughout; each scenario writes its config, waits for the
# reload, and has the probe's fake mouse draw over the two windows start.sh
# opens. Gestures run `note WORDS`, which logs WORDS and the window's side to
# ~/out.txt; the windows log the keys typed into them there too. The screenshot
# scenes ask the host for shots mid-stroke, for vm.sh's report.
set -u
export XDG_RUNTIME_DIR=/run/user/1000 PATH=~/bin:$PATH
C=${1:?compositor}
LOG=~/mousewand.log

display=$(bash ~/start.sh "$C") || { echo "Bail out! $C didn't start"; exit 1; }
case "$display" in
  :*) export DISPLAY=$display; unset WAYLAND_DISPLAY ;;
  *) export WAYLAND_DISPLAY=$display ;;
esac
dispatch=
case "$C" in
  hyprland) export MOUSEWAND_BACKEND=hyprland; dispatch='hl.dsp.window.fullscreen_state({ internal = 1, client = 0, action = "set" })' ;;
  sway) dispatch="fullscreen toggle" ;;
  kwin) dispatch="Window Maximize" ;;
  niri) export MOUSEWAND_BACKEND=niri; dispatch=maximize-column ;;
  labwc | cosmic) export MOUSEWAND_BACKEND=toplevel ;;
esac

cat > ~/bin/note <<'EOF'
#!/bin/sh
# note WORDS: logs WORDS and which of the suite's windows the gesture was over.
case "$MOUSEWAND_CLASS" in
  foot | Foot) side=left ;;
  other | Other) side=right ;;
  *) side="class=$MOUSEWAND_CLASS" ;;
esac
echo "$* $side" >> ~/out.txt
EOF
chmod +x ~/bin/note

# --- Writing the config

BASE='devices = ["mousewand test mouse"]
open_settings = { stroke = "LRLRLR" }'

# config: config.toml from stdin, after BASE, and no app files.
config() {
  rm -rf ~/cfg/apps ~/cfg/themes ~/cfg/shapes
  mkdir -p ~/cfg/apps ~/cfg/themes
  { echo "$BASE"; cat; } > ~/cfg/config.toml
}

# file PATH: a file under ~/cfg, from stdin.
file() { cat > ~/cfg/"$1"; }

reloads() { grep -cE "config reloaded|keeping the last good config" "$LOG"; }

# loaded: waits until mousewand has read the files written since the last call,
# and no change is still on its way.
loaded() {
  local seen=$(reloads) t
  for t in $(seq 50); do
    sleep 0.1
    [ "$(reloads)" -gt "$seen" ] && break
  done
  while sleep 0.5; [ "$(reloads)" -gt "$seen" ]; do seen=$(reloads); done
  : > ~/out.txt
  MARK=$(wc -l < "$LOG")
}

# --- Drawing

# The cursor to the left window's middle, at (400, 440): far up and left
# first, which every compositor clamps to the corner. Through the right window
# on the way, so the left one is entered and gets focus where focus follows
# the mouse.
PARK="move 40 0 40; wait 100; move -40 0 60; move 0 -40 40; move 0 4 110; move 8 0 50; wait 200;"
RIGHT="move 40 0 20; wait 200;"
LEFT="move -40 0 20; wait 200;"
# One 150 px stroke with the trigger, then back to where it began without it.
stroke() { echo "down; move $1 $2 50; up; wait 400; move $((-$1)) $((-$2)) 50; wait 150;"; }
L=$(stroke -3 0) R=$(stroke 3 0) U=$(stroke 0 -3) D=$(stroke 0 3)

# draw SCRIPT: the fake mouse plays SCRIPT from the park, then the actions finish.
draw() {
  probe play "$PARK $1" > /dev/null || echo "# probe failed"
  sleep 0.6
}

# --- Checking

N=0 FAILED=0
pass() { N=$((N + 1)); echo "ok $N - $1"; }
fail() {
  N=$((N + 1)) FAILED=$((FAILED + 1))
  echo "not ok $N - $1"
  shift
  printf '#   %s\n' "$@"
}
skip() { N=$((N + 1)); echo "ok $N - $1 # SKIP $2"; }

# expect NAME LINE...: out.txt must hold exactly these lines, in order.
expect() {
  local name=$1 want got
  shift
  want=$(printf '%s\n' "$@")
  got=$(cat ~/out.txt)
  [ "$got" = "${want%$'\n'}" ] && pass "$name" ||
    fail "$name" "want: $(printf '%s | ' "$@")" "got:  $(tr '\n' '|' < ~/out.txt | sed 's/|/ | /g')"
}

# expect_count NAME LINE MIN MAX: LINE appears MIN to MAX times.
expect_count() {
  local n=$(grep -cxF "$2" ~/out.txt)
  [ "$n" -ge "$3" ] && [ "$n" -le "$4" ] && pass "$1 ($n)" || fail "$1" "want $3 to $4 of \"$2\", got $n"
}

# logged PATTERN: mousewand logged a line matching PATTERN since loaded.
logged() { tail -n +"$((MARK + 1))" "$LOG" | grep -qE "$1"; }

# --- Scenarios

per_app() {
  config <<'EOF'
[[gesture]]
stroke = "L"
action = { command = "note L-global" }
EOF
  file apps/other.toml <<'EOF'
[[gesture]]
stroke = "L"
action = { command = "note L-other" }
EOF
  loaded
  draw "$L $RIGHT $L"
  expect "an app's gesture replaces the global one, for the window under the cursor" "L-global left" "L-other right"
}

env_vars() {
  config <<'EOF'
[[gesture]]
stroke = "R"
action = { command = 'note "title=$MOUSEWAND_TITLE"' }
EOF
  loaded
  draw "$R $RIGHT $R"
  expect "commands get the window's class and title" "title=left left" "title=right right"
}

keys() {
  config <<'EOF'
EOF
  file apps/foot.toml <<'EOF'
[[gesture]]
stroke = "D"
action = [{ keys = "a" }, { keys = "shift+b" }]
EOF
  loaded
  draw "$D $RIGHT $D"
  expect "keys go to the window drawn over, only where they're set" "key left a" "key left B"
}

sequence() {
  config <<'EOF'
[[gesture]]
stroke = "U"
action = [{ command = "note one" }, { wait = 300 }, { command = "note two" }]
EOF
  loaded
  draw "$U"
  expect "a list of steps runs in order" "one left" "two left"
}

repeat() {
  config <<'EOF'
[[gesture]]
stroke = "R"
repeat = true
action = { command = "note tick" }
EOF
  loaded
  # From x = 100, 450 px right: recognized after the first ~30, then every 80.
  draw "move -3 0 100; down; move 3 0 150; up; wait 300"
  expect_count "a repeating stroke fires again every step" "tick left" 4 6
}

hold() {
  config <<'EOF'
EOF
  file apps/foot.toml <<'EOF'
[[gesture]]
stroke = "U"
action = { keys = "a", hold = "shift" }

[[gesture]]
stroke = "D"
action = { keys = "a" }
EOF
  loaded
  draw "down; move 0 -3 100; up; wait 400; move 0 3 100; wait 150; $D"
  expect_count "hold keeps its keys down while the stroke repeats" "key left A" 2 5
  [ "$(tail -1 ~/out.txt)" = "key left a" ] && pass "hold lets go of its keys on release" ||
    fail "hold lets go of its keys on release" "last line: $(tail -1 ~/out.txt)"
}

wheel() {
  config <<'EOF'
[[gesture]]
wheel = "up"
action = { command = "note wheel-up" }

[[gesture]]
wheel = "down"
action = { command = "note wheel-down" }
EOF
  loaded
  draw "down; wait 50; wheel 1; wait 100; wheel 1; wait 100; wheel -1; wait 100; up; wait 300"
  expect "the wheel with the trigger held fires on every notch" "wheel-up left" "wheel-up left" "wheel-down left"
}

rocker() {
  config <<'EOF'
[[gesture]]
click = "left"
action = { command = "note rocker" }
EOF
  loaded
  draw "down; wait 50; press left; wait 30; release left; wait 50; up; wait 300"
  expect "a click with the trigger held fires" "rocker left"
}

shape() {
  config <<'EOF'
[[gesture]]
shape = "circle"
action = { command = "note circle" }

[[gesture]]
stroke = "U"
action = { command = "note stroke" }
EOF
  loaded
  draw "down; circle 110; up; wait 400"
  expect "a drawn circle fires its shape" "circle left"
}

modifiers() {
  config <<'EOF'
[[gesture]]
stroke = "L"
action = { command = "note plain" }

[[gesture]]
stroke = "L"
with = "shift"
action = { command = "note shifted" }
EOF
  loaded
  # mousewand finds the probe's keyboard at its next scan, within 5 s.
  draw "wait 5500; press shift; $L release shift; wait 200; $L"
  expect "a stroke with a modifier held does something else" "shifted left" "plain left"
}

disabled() {
  config <<'EOF'
[[gesture]]
stroke = "L"
action = { command = "note L" }
EOF
  file apps/other.toml <<'EOF'
enabled = false
EOF
  loaded
  draw "$L $RIGHT $L"
  expect "enabled = false turns gestures off over that app" "L left"
}

toggle() {
  config <<'EOF'
[[gesture]]
stroke = "L"
action = { command = "note L" }
EOF
  loaded
  draw "$L run mousewand toggle; wait 300; $L run mousewand toggle; wait 300; $L"
  expect "mousewand toggle pauses gestures and resumes them" "L left" "L left"
}

reload() {
  config <<'EOF'
[[gesture]]
stroke = "L"
action = { command = "note first" }
EOF
  loaded
  draw "$L"
  local first=$(cat ~/out.txt)
  config <<'EOF'
[[gesture]]
stroke = "L"
action = { command = "note second" }
EOF
  loaded
  draw "$L"
  local second=$(cat ~/out.txt)
  echo 'stroke = ' >> ~/cfg/config.toml
  loaded
  draw "$L"
  printf '%s\n%s\n%s\n' "$first" "$second" "$(cat ~/out.txt)" > ~/out.txt
  expect "a saved config applies, and a broken one keeps the last good one" "first left" "second left" "second left"
}

global_trigger() {
  config <<'EOF'
[[gesture]]
stroke = "L"
action = { command = "note app" }

[global]
trigger = "back"

[[global.gesture]]
stroke = "L"
action = { command = "note global" }
EOF
  loaded
  draw "press side; move -3 0 50; release side; wait 400; move 3 0 50; wait 150; $L"
  expect "global gestures on their own trigger leave the main one to the apps" "global left" "app left"
}

cancel() {
  config <<'EOF'
cancel_ms = 500

[[gesture]]
stroke = "L"
action = { command = "note L" }
EOF
  loaded
  draw "down; move -3 0 50; wait 900; up; wait 300"
  expect "holding still mid-stroke cancels it"
  logged "cancelled" && pass "the cancel is logged" || fail "the cancel is logged" "no \"cancelled\" in the log"
}

# The virtual mouse must pass a quick click on, and a press held still.
replay() {
  config <<'EOF'
[[gesture]]
stroke = "L"
action = { command = "note L" }
EOF
  loaded
  probe capture 6 > ~/capture.txt &
  local capture=$!
  draw "down; wait 30; up; wait 500; down; wait 600; up; wait 300"
  wait $capture
  local presses=$(awk '$3 == 274 { printf "%s ", $4 }' ~/capture.txt)
  [ "$presses" = "1 0 1 0 " ] && pass "a click and a still press reach the desktop as the middle button" ||
    fail "a click and a still press reach the desktop as the middle button" "BTN_MIDDLE values: ${presses:-none}"
}

selection() {
  config <<'EOF'
[[gesture]]
stroke = "U"
action = { command = 'note "sel=$SELECTION"' }
EOF
  loaded
  # Both stay running to serve the selection; with the suite's output, they
  # would hold vm.sh's ssh open.
  if [ -n "${DISPLAY:-}" ] && [ -z "${WAYLAND_DISPLAY:-}" ]; then
    printf 'hello wand' | xclip -selection primary > /dev/null 2>&1
  else
    wl-copy --primary 'hello wand' > /dev/null 2>&1
  fi
  draw "$U"
  expect "\$SELECTION is the primary selection" "sel=hello wand left"
  pkill -x wl-copy
  pkill -x xclip
}

# Shots of the trail and the hint, for the report to compare with Hyprland's:
# each before the stroke and while it's held, so their difference is what
# mousewand drew. A theme whose trail lingers keeps the shot's delay out of it.
screens() {
  config <<'EOF'
theme = "still"

# A background that stands out from the terminals', so the whole hint counts
# as drawn, not just its glyphs.
[hint]
enabled = true
background = "#4c5a78"

[[gesture]]
stroke = "D"
name = "Focus"
action = { keys = "a" }

[[gesture]]
stroke = "DR"
name = "Down right"
action = { command = "note shot" }
EOF
  file themes/still.toml <<'EOF'
color = "#73bfff"
rejected = "#ff5959"
width = 26
life_ms = 4000
fade_ms = 300
glow = 0.6
EOF
  loaded
  # D first, so the window has focus and any dimming has settled.
  draw "$D wait 1500;
    run shot $C-trail-before; down; move 0 3 60; move 3 0 80; wait 150; run shot $C-trail-mid; up; wait 300;
    move -3 0 80; move 0 -3 60; wait 2500;
    run shot $C-nomatch-before; down; move 0 -3 60; move -3 0 60; wait 150; run shot $C-nomatch-mid; up"
  local missing=() s
  for s in trail-before trail-mid nomatch-before nomatch-mid; do
    grep -q "^shot $s$" ~/shots.txt 2> /dev/null || missing+=("$s")
  done
  [ ${#missing[@]} = 0 ] && pass "screenshots of the trail and the hint" ||
    fail "screenshots of the trail and the hint" "missing: ${missing[*]} (is the VM booted with vm.sh's shot channel?)"
}

dispatch() {
  [ -n "$dispatch" ] || { skip "dispatch runs a compositor command" "$C takes none"; return; }
  config <<EOF
[[gesture]]
stroke = "U"
action = { dispatch = '$dispatch' }
EOF
  loaded
  draw "$U"
  logged "Dispatch" && ! logged "ERROR" && pass "dispatch runs a compositor command" ||
    fail "dispatch runs a compositor command" "$(tail -n +"$((MARK + 1))" "$LOG" | grep -E "Dispatch|ERROR" | cut -c25- | head -3)"
}

# --- Running

cat > ~/bin/shot <<'EOF'
#!/usr/bin/env bash
# shot NAME: the host saves the VM's display as NAME.png (vm.sh's shot.py).
exec 3<> /dev/tcp/10.0.2.100/4444 || exit 1
echo "$1" >&3
read -r -t 5 reply <&3
[ "$reply" = ok ] && echo "shot ${1#*-}" >> ~/shots.txt
EOF
chmod +x ~/bin/shot
rm -f ~/shots.txt

mkdir -p ~/cfg
config < /dev/null
: > "$LOG"
MOUSEWAND_CONFIG_DIR=~/cfg mousewand > "$LOG" 2>&1 &
MOUSEWAND=$!
sleep 3
# KWin and X apply their own acceleration to mousewand's virtual mouse.
case "$C" in
  kwin)
    for dev in $(busctl --user tree org.kde.KWin --list | grep "/InputDevice/event"); do
      busctl --user get-property org.kde.KWin "$dev" org.kde.KWin.InputDevice name | grep -q "mousewand virtual mouse" &&
        busctl --user set-property org.kde.KWin "$dev" org.kde.KWin.InputDevice pointerAccelerationProfileFlat b true
    done
    ;;
  x11) xinput set-prop "mousewand virtual mouse" "libinput Accel Profile Enabled" 0 1 0 2> /dev/null ;;
esac

echo "TAP version 13"
echo "# $C on $display"
for scenario in ${SCENARIOS:-per_app env_vars keys sequence repeat hold wheel rocker shape modifiers disabled \
  toggle reload global_trigger cancel replay selection screens dispatch}; do
  $scenario
done
echo "1..$N"

kill $MOUSEWAND
wait $MOUSEWAND
if grep -qE "ERROR|panicked" "$LOG"; then
  echo "# mousewand logged:"
  grep -E "ERROR|WARN|panicked" "$LOG" | cut -c25- | sort | uniq -c | sed 's/^/#   /'
fi
[ "$FAILED" = 0 ]
