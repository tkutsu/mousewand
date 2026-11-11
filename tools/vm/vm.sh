#!/usr/bin/env bash
# A throwaway Arch VM for testing wisp on compositors other than the host's, end to end:
# a fake mouse inside the guest, wisp's grab, the compositor's libinput.
#
#   tools/vm/vm.sh setup                download the image, make disk and keys
#   tools/vm/vm.sh start                boot; the first boot provisions the guest
#   tools/vm/vm.sh test COMPOSITOR      build wisp, run the guest test on sway, kwin,
#                                       gnome, niri, wayfire, labwc, cosmic, river or x11
#   tools/vm/vm.sh shot OUT.png         screenshot the guest display
#   tools/vm/vm.sh ssh [CMD...]         a shell in the guest, as user wisp
#   tools/vm/vm.sh stop                 power off
#
# The VM lives in $WISP_VM_DIR (default ~/.cache/wisp-vm). Needs qemu with KVM.
set -eu
HERE=$(cd "$(dirname "$0")" && pwd)
ROOT=$(cd "$HERE/../.." && pwd)
D=${WISP_VM_DIR:-$HOME/.cache/wisp-vm}
IMAGE=https://geo.mirror.pkgbuild.com/images/latest/Arch-Linux-x86_64-cloudimg.qcow2
OPTS=(-i "$D/id_vm" -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null -o ConnectTimeout=5)
SSH=(ssh -q "${OPTS[@]}" -p 2222 wisp@127.0.0.1)

put() { scp -q "${OPTS[@]}" -P 2222 "${@:1:$#-1}" "wisp@127.0.0.1:${!#}"; }

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
  pgrep -f "http.server 8731" > /dev/null ||
    (python3 -I -m http.server 8731 --bind 127.0.0.1 --directory "$D/seed" > "$D/http.log" 2>&1 &)
  qemu-system-x86_64 -enable-kvm -cpu host -smp 6 -m 8G \
    -drive file="$D/disk.qcow2",if=virtio \
    -vga none -device bochs-display,xres=1600,yres=900 -display none \
    -nic user,model=virtio-net-pci,hostfwd=tcp:127.0.0.1:2222-:22 \
    -smbios "type=1,serial=ds=nocloud;s=http://10.0.2.2:8731/" \
    -qmp unix:"$D/qmp.sock",server=on,wait=off \
    -serial file:"$D/serial.log" -pidfile "$D/vm.pid" -daemonize
  wait_ssh
}

provision() {
  echo "provisioning (installs the compositors; takes a few minutes)"
  "${SSH[@]}" 'sudo pacman -Syu --noconfirm --needed sway kwin gnome-shell niri cosmic-comp wayfire labwc river \
    xorg-server xorg-xinit xorg-xauth xf86-input-libinput xorg-xinput xfwm4 xterm xdotool \
    foot mesa vulkan-swrast xorg-xwayland jq > /tmp/pacman.log 2>&1'
  put "$ROOT/dist/70-wisp.rules" "$ROOT/dist/wisp-uinput.conf" "$HERE"/guest-*.sh "$HERE/kwin-layout.js" \
    "$HERE/wayfire.ini" ""
  "${SSH[@]}" 'set -e
    sudo cp 70-wisp.rules /etc/udev/rules.d/
    sudo cp wisp-uinput.conf /etc/modules-load.d/
    for f in guest-*.sh; do mv "$f" "${f#guest-}"; done
    chmod +x session.sh
    echo none > compositor
    mkdir -p bin cfg/apps ext
    echo "[ \"\$(tty)\" = /dev/tty1 ] && exec ~/session.sh" >> .bash_profile
    sudo mkdir -p /etc/systemd/system/getty@tty1.service.d
    printf "[Service]\nExecStart=\nExecStart=-/sbin/agetty --autologin wisp --noclear %%I \$TERM\n" |
      sudo tee /etc/systemd/system/getty@tty1.service.d/autologin.conf > /dev/null
    gsettings set org.gnome.desktop.interface enable-hot-corners false 2> /dev/null || true
    touch .provisioned'
  # The upgrade may have replaced the kernel.
  "${SSH[@]}" 'sudo systemctl reboot' || true
  sleep 10
  wait_ssh
}

case "${1:-}" in
setup)
  mkdir -p "$D/seed"
  cd "$D"
  curl -fsSLO "$IMAGE" && curl -fsSLO "$IMAGE.SHA256" && sha256sum -c "$(basename "$IMAGE").SHA256"
  [ -e id_vm ] || ssh-keygen -q -t ed25519 -N "" -C wisp-vm -f id_vm
  qemu-img create -q -f qcow2 -b "$(basename "$IMAGE")" -F qcow2 disk.qcow2 40G
  printf 'instance-id: wisp-vm\nlocal-hostname: wisp-vm\n' > seed/meta-data
  printf '#cloud-config\nusers:\n  - name: wisp\n    groups: [wheel, input, video]\n    sudo: "ALL=(ALL) NOPASSWD:ALL"\n    shell: /bin/bash\n    ssh_authorized_keys:\n      - %s\n' \
    "$(cat id_vm.pub)" > seed/user-data
  echo "ready: $D"
  ;;
start)
  boot
  "${SSH[@]}" 'test -e .provisioned' || provision
  echo up
  ;;
test)
  compositor=${2:?sway, kwin, gnome, niri, wayfire, labwc, cosmic, river or x11}
  (cd "$ROOT" && cargo build -q -p wisp && cargo build -q -p wisp-input --example probe)
  put "$ROOT/target/debug/wisp" "$ROOT/target/debug/examples/probe" bin/
  put "$ROOT/dist/gnome-extension/wisp@tkutsu.github.io/"* ext/
  case "$compositor" in
    sway)
      "${SSH[@]}" 'bash switch.sh sway > /dev/null
        S=$(ls /run/user/1000/sway-ipc.*.sock)
        SWAYSOCK=$S swaymsg -q "exec foot"; sleep 1; SWAYSOCK=$S swaymsg -q "exec foot --app-id=other"; sleep 2
        DISPATCH="fullscreen toggle" bash test.sh wayland-1'
      ;;
    kwin)
      "${SSH[@]}" 'bash kwin-setup.sh; DISPATCH="Window Maximize" bash test.sh wayland-0'
      ;;
    gnome)
      "${SSH[@]}" 'bash gnome-setup.sh restart; bash gnome-setup.sh window foot; bash test.sh wayland-0
        bash gnome-setup.sh window other; bash test.sh wayland-0'
      ;;
    # These run nested in sway: niri and COSMIC refuse llvmpipe on the bare
    # display, and river 0.5 needs a separate window manager to show windows.
    niri) "${SSH[@]}" 'd=$(bash nested.sh niri); WISP_BACKEND=niri DISPATCH=maximize-column bash test.sh $d' ;;
    wayfire) "${SSH[@]}" 'd=$(bash nested.sh wayfire); WISP_BACKEND=wayfire bash test.sh $d' ;;
    labwc | cosmic | river)
      "${SSH[@]}" "d=\$(bash nested.sh $compositor); WISP_BACKEND=toplevel bash test.sh \$d"
      ;;
    # xfwm4 with its compositor. Xorg here applies uinput motion in bursts,
    # so this checks window lookup and actions; the trail needs smooth motion.
    x11) "${SSH[@]}" 'bash x11-setup.sh; bash test.sh :0' ;;
    *) echo "unknown compositor $compositor" >&2; exit 1 ;;
  esac
  ;;
shot)
  qmp "{\"execute\": \"screendump\", \"arguments\": {\"filename\": \"$D/shot.png\", \"format\": \"png\"}}" > /dev/null
  sleep 0.5
  cp "$D/shot.png" "${2:?output file}"
  ;;
ssh) shift; "${SSH[@]}" "$@" ;;
stop)
  qmp '{"execute": "system_powerdown"}' > /dev/null || true
  for _ in $(seq 20); do [ -e "/proc/$(cat "$D/vm.pid" 2> /dev/null || echo 0)" ] || break; sleep 1; done
  pkill -f "http.server 8731" || true
  ;;
*) sed -n '2,12p' "$0" | sed 's/^# \?//'; exit 1 ;;
esac
