#!/usr/bin/env bash
# A throwaway Arch VM for testing mousewand on compositors other than the host's, end to end:
# a fake mouse inside the guest, mousewand's grab, the compositor's libinput.
#
#   tools/vm/vm.sh setup                download the image, make disk and keys
#   tools/vm/vm.sh start                boot; the first boot provisions the guest
#   tools/vm/vm.sh test [COMPOSITOR...] build mousewand, run the feature suite on each of
#                                       hyprland, sway, kwin, gnome, niri, labwc, cosmic and
#                                       x11 (all by default), then compare their screenshots
#                                       with Hyprland's. SCENARIOS="keys screens" runs only
#                                       those (guest-suite.sh lists them)
#   tools/vm/vm.sh report              compare the screenshots again, without a run
#   tools/vm/vm.sh shot OUT.png         screenshot the guest display
#   tools/vm/vm.sh ssh [CMD...]         a shell in the guest, as user mousewand
#   tools/vm/vm.sh stop                 power off
#
# The VM lives in $MOUSEWAND_VM_DIR (default ~/.cache/mousewand-vm), and so do each
# compositor's results, screenshots and report.html. Needs qemu with KVM.
# $MOUSEWAND_VM_PORT (default 2222) is its SSH port on localhost; the cloud-init
# seed is served on the port 6509 above it, so two VMs can run side by side.
set -eu
HERE=$(cd "$(dirname "$0")" && pwd)
ROOT=$(cd "$HERE/../.." && pwd)
D=${MOUSEWAND_VM_DIR:-$HOME/.cache/mousewand-vm}
IMAGE=https://geo.mirror.pkgbuild.com/images/latest/Arch-Linux-x86_64-cloudimg.qcow2
OPTS=(-i "$D/id_vm" -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null -o ConnectTimeout=5)
PORT=${MOUSEWAND_VM_PORT:-2222}
SEED=$((PORT + 6509))
SSH=(ssh -q "${OPTS[@]}" -p "$PORT" mousewand@127.0.0.1)
COMPOSITORS=(hyprland sway kwin gnome niri labwc cosmic x11)
# Changing this list provisions the guest again on the next start.
PACKAGES="sway kwin gnome-shell niri cosmic-comp labwc hyprland \
  xorg-server xorg-xinit xorg-xauth xf86-input-libinput xorg-xinput xfwm4 xterm xdotool picom \
  foot mesa vulkan-swrast xorg-xwayland jq wl-clipboard xclip"

put() { scp -q "${OPTS[@]}" -P "$PORT" "${@:1:$#-1}" "mousewand@127.0.0.1:${!#}"; }

qmp() {
  python3 -I - "$D/qmp.sock" "$1" <<'EOF'
import json, socket, sys
s = socket.socket(socket.AF_UNIX)
s.connect(sys.argv[1])
f = s.makefile("rw")
f.readline()
for cmd in ({"execute": "qmp_capabilities"}, json.loads(sys.argv[2])):
    f.write(json.dumps(cmd) + "\n"); f.flush()
    while "event" in (reply := json.loads(f.readline())):
        pass
print(json.dumps(reply))
EOF
}

wait_ssh() {
  for _ in $(seq 60); do
    "${SSH[@]}" true 2> /dev/null && return 0
    sleep 3
  done
  echo "no ssh after 3 minutes" >&2
  return 1
}

boot() {
  pgrep -f "http.server $SEED" > /dev/null ||
    (python3 -I -m http.server "$SEED" --bind 127.0.0.1 --directory "$D/seed" > "$D/http.log" 2>&1 &)
  # The guest's `shot NAME` connects to 10.0.2.100:4444, and QEMU runs shot.py
  # for it. A copy, so the VM outlives the checkout it was started from.
  mkdir -p "$D/shots"
  cp "$HERE/shot.py" "$D/shot.py"
  local shot="python3 -I $D/shot.py $D $D/shots"
  qemu-system-x86_64 -enable-kvm -cpu host -smp 6 -m 8G \
    -drive file="$D/disk.qcow2",if=virtio \
    -vga none -device bochs-display,xres=1600,yres=900 -display none \
    -nic "user,model=virtio-net-pci,hostfwd=tcp:127.0.0.1:$PORT-:22,guestfwd=tcp:10.0.2.100:4444-cmd:${shot//,/,,}" \
    -smbios "type=1,serial=ds=nocloud;s=http://10.0.2.2:$SEED/" \
    -qmp unix:"$D/qmp.sock",server=on,wait=off \
    -serial file:"$D/serial.log" -pidfile "$D/vm.pid" -daemonize
  wait_ssh
}

provision() {
  echo "provisioning (installs the compositors; takes a few minutes)"
  "${SSH[@]}" "sudo pacman -Syu --noconfirm --needed $PACKAGES > /tmp/pacman.log 2>&1"
  put "$ROOT/dist/70-mousewand.rules" "$ROOT/dist/mousewand-uinput.conf" "$HERE/guest-session.sh" ""
  "${SSH[@]}" 'set -e
    sudo cp 70-mousewand.rules /etc/udev/rules.d/
    sudo cp mousewand-uinput.conf /etc/modules-load.d/
    mv guest-session.sh session.sh
    chmod +x session.sh
    echo none > compositor
    mkdir -p bin cfg/apps ext
    grep -q session.sh .bash_profile || echo "[ \"\$(tty)\" = /dev/tty1 ] && exec ~/session.sh" >> .bash_profile
    sudo mkdir -p /etc/systemd/system/getty@tty1.service.d
    printf "[Service]\nExecStart=\nExecStart=-/sbin/agetty --autologin mousewand --noclear %%I \$TERM\n" |
      sudo tee /etc/systemd/system/getty@tty1.service.d/autologin.conf > /dev/null
    gsettings set org.gnome.desktop.interface enable-hot-corners false 2> /dev/null || true'
  echo "$PACKAGES" | "${SSH[@]}" 'cat > .provisioned'
  # The upgrade may have replaced the kernel.
  "${SSH[@]}" 'sudo systemctl reboot' || true
  sleep 10
  wait_ssh
}

# The guest scripts, copied on every run so edits apply without provisioning.
sync() {
  put "$HERE"/guest-*.sh "$HERE/kwin-layout.js" "$HERE/hyprland.lua" ""
  put -r "$HERE/gnome-tile" ext/
  "${SSH[@]}" 'set -e
    # tty1 runs the session loop; a new one takes over when getty logs in again.
    if ! cmp -s guest-session.sh session.sh; then
      mv guest-session.sh session.sh
      chmod +x session.sh
      sudo pkill -t tty1 || true
    fi
    rm -f guest-session.sh
    for f in guest-*.sh; do mv "$f" "${f#guest-}"; done
    t=.local/share/gnome-shell/extensions/mousewand-tile@test
    mkdir -p "$t" .config/hypr/configs
    cp ext/gnome-tile/* "$t/"
    mv hyprland.lua .config/hypr/'
  # Your Hyprland look, if there is one; the VM's Hyprland is the baseline the
  # other compositors' screenshots are compared with.
  local look=() f
  for f in settings decoration animation; do
    [ -e "$HOME/.config/hypr/configs/$f.lua" ] && look+=("$HOME/.config/hypr/configs/$f.lua")
  done
  [ ${#look[@]} -gt 0 ] && put "${look[@]}" .config/hypr/configs/
  return 0
}

report() {
  (cd "$ROOT" && cargo run -q -p mousewand-shots -- "$D/shots" "$D/report.html")
}

case "${1:-}" in
setup)
  mkdir -p "$D/seed"
  cd "$D"
  curl -fsSLO "$IMAGE" && curl -fsSLO "$IMAGE.SHA256" && sha256sum -c "$(basename "$IMAGE").SHA256"
  [ -e id_vm ] || ssh-keygen -q -t ed25519 -N "" -C mousewand-vm -f id_vm
  qemu-img create -q -f qcow2 -b "$(basename "$IMAGE")" -F qcow2 disk.qcow2 40G
  printf 'instance-id: mousewand-vm\nlocal-hostname: mousewand-vm\n' > seed/meta-data
  printf '#cloud-config\nusers:\n  - name: mousewand\n    groups: [wheel, input, video]\n    sudo: "ALL=(ALL) NOPASSWD:ALL"\n    shell: /bin/bash\n    ssh_authorized_keys:\n      - %s\n' \
    "$(cat id_vm.pub)" > seed/user-data
  echo "ready: $D"
  ;;
start)
  boot
  [ "$("${SSH[@]}" 'cat .provisioned 2> /dev/null')" = "$PACKAGES" ] || provision
  echo up
  ;;
test)
  shift
  [ $# -gt 0 ] || set -- "${COMPOSITORS[@]}"
  (cd "$ROOT" && cargo build -q -p mousewand && cargo build -q -p mousewand-input --example probe)
  put "$ROOT/target/debug/mousewand" "$ROOT/target/debug/examples/probe" bin/
  sync
  mkdir -p "$D/results"
  failed=()
  for c in "$@"; do
    echo "=== $c"
    # Only a run that takes new shots drops the old ones.
    case " ${SCENARIOS:-screens} " in *" screens "*) rm -f "$D/shots/$c"-*.png ;; esac
    "${SSH[@]}" "SCENARIOS='${SCENARIOS:-}' bash suite.sh $c" | tee "$D/results/$c.txt"
    [ "${PIPESTATUS[0]}" = 0 ] || failed+=("$c")
  done
  report || failed+=(screenshots)
  [ ${#failed[@]} = 0 ] || { echo "failed: ${failed[*]}"; exit 1; }
  ;;
report) report ;;
shot)
  qmp "{\"execute\": \"screendump\", \"arguments\": {\"filename\": \"$D/shot.png\", \"format\": \"png\"}}" > /dev/null
  sleep 0.5
  cp "$D/shot.png" "${2:?output file}"
  ;;
ssh) shift; "${SSH[@]}" "$@" ;;
stop)
  qmp '{"execute": "system_powerdown"}' > /dev/null || true
  for _ in $(seq 20); do [ -e "/proc/$(cat "$D/vm.pid" 2> /dev/null || echo 0)" ] || break; sleep 1; done
  pkill -f "http.server $SEED" || true
  ;;
*) sed -n '2,20p' "$0" | sed 's/^# \?//'; exit 1 ;;
esac
