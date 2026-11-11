#!/usr/bin/env bash
# Runs inside the guest: wisp against a fake mouse on the current compositor.
# Usage: guest-test.sh DISPLAY [screenshot-hook] — a Wayland display such as
# wayland-1, or an X11 one such as :0.
set -u
export XDG_RUNTIME_DIR=/run/user/1000
case "$1" in
  :*) export DISPLAY=$1; unset WAYLAND_DISPLAY ;;
  *) export WAYLAND_DISPLAY=$1 ;;
esac
export PATH=~/bin:$PATH
rm -f ~/out.txt
cat > ~/cfg/config.toml <<'EOF'
devices = ["wisp test mouse"]

[[gesture]]
stroke = "L"
action = { command = "echo \"L global $WISP_CLASS\" >> ~/out.txt" }
EOF
if [ -n "${DISPATCH:-}" ]; then
  printf '\n[[gesture]]\nstroke = "U"\naction = { dispatch = "%s" }\n' "$DISPATCH" >> ~/cfg/config.toml
fi
cat > ~/cfg/apps/other.toml <<'EOF'
[[gesture]]
stroke = "L"
action = { command = "echo \"L other $WISP_CLASS\" >> ~/out.txt" }
EOF
cat > ~/cfg/apps/foot.toml <<'EOF'
match = ["foot", "XTerm"]

[[gesture]]
stroke = "D"
action = { keys = "a" }
EOF

WISP_CONFIG_DIR=~/cfg ~/bin/wisp > ~/wisp.log 2>&1 &
WISP=$!
sleep 3
# X applies its own pointer acceleration; flat keeps the moves below predictable.
if [ -n "${DISPLAY:-}" ] && [ -z "${WAYLAND_DISPLAY:-}" ]; then
  xinput set-prop "wisp virtual mouse" "libinput Accel Profile Enabled" 0 1 0 2> /dev/null
fi
# Park the cursor at the left edge, mid-height, then draw over each half.
probe play "move -40 0 60; move 0 -40 40; move 0 4 110; move 8 0 50; wait 200;
  down; move -3 0 50; up; wait 400;
  move 40 0 20; wait 200;
  down; move -3 0 50; up; wait 400;
  move -40 0 20; wait 200;
  down; move 0 3 30; ${2:-} move 0 3 10; up; wait 600;
  move 40 0 20; wait 200;
  down; move 0 -3 40; up; wait 1000"
sleep 1
kill $WISP
wait $WISP
echo "--- out"; cat ~/out.txt
echo "--- wisp log"; grep -vE "grabbed|released" ~/wisp.log
