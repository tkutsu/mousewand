"""Screenshots the VM for the guest, mid-gesture.

QEMU runs this for each connection the guest makes to 10.0.2.100:4444
(vm.sh's guestfwd). The guest sends a name; this saves the VM's display to
DIR/NAME.png through QMP and answers "ok". An SSH round trip would be too
slow: the trail moves on while it waits.

Usage: shot.py VM_DIR SHOTS_DIR
"""

import json
import re
import socket
import sys

vm_dir, shots = sys.argv[1], sys.argv[2]
name = sys.stdin.readline().strip()
if not re.fullmatch(r"[a-z0-9][a-z0-9_.-]*", name):
    print("bad name", flush=True)
    sys.exit(1)

s = socket.socket(socket.AF_UNIX)
s.connect(f"{vm_dir}/qmp.sock")
f = s.makefile("rw")
f.readline()
for cmd in (
    {"execute": "qmp_capabilities"},
    {"execute": "screendump", "arguments": {"filename": f"{shots}/{name}.png", "format": "png"}},
):
    f.write(json.dumps(cmd) + "\n")
    f.flush()
    while "event" in (reply := json.loads(f.readline())):
        pass
print("ok" if "return" in reply else f"failed: {reply}", flush=True)
