#!/usr/bin/env bash
# Generate the synthetic sysfs/procfs fixture trees under fixtures/.
# All data is invented; see fixtures/README.md. Idempotent: overwrites in
# place.
#
# Scenarios `basic-workstation`, `virtual-interfaces` and `procfs/router-ish`
# are faithful ports of the original WO-01 fixture script (recovered from the
# 2026-11-18 persistence incident); `combined-laptop` and `empty` were added
# during WO-01R reconstruction for full-provider-run and empty-observation
# coverage.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)/fixtures"

# Safety (WO-03-F / AUD-04): this script writes a fixed set of paths under
# the repository's fixtures/ tree ONLY. Refuse to run anywhere else — a
# stray copy must never overwrite unrelated data.
if [ ! -f "$(dirname "$0")/../Cargo.toml" ] || [ ! -d "$ROOT/sysfs" ]; then
  echo "gen-sysfs-fixtures.sh: refusing to run — '$ROOT' is not a repository fixtures tree" >&2
  exit 1
fi

write() { # write <file> <content>
  mkdir -p "$(dirname "$1")"
  printf '%s\n' "$2" > "$1"
}

iface() { # iface <dir> <ifindex> <operstate> <address> <mtu> <flags> <type>
  local d="$1"
  write "$d/ifindex"   "$2"
  write "$d/operstate" "$3"
  write "$d/address"   "$4"
  write "$d/mtu"       "$5"
  write "$d/flags"     "$6"
  write "$d/type"      "$7"
  mkdir -p "$d/statistics"
}

stats() { # stats <dir> <rx_bytes> <tx_bytes> <rx_packets> <tx_packets>
  write "$1/statistics/rx_bytes"   "$2"
  write "$1/statistics/tx_bytes"   "$3"
  write "$1/statistics/rx_packets" "$4"
  write "$1/statistics/tx_packets" "$5"
}

# --- fixture: basic-workstation (original WO-01 data) ----------------------
BW="$ROOT/sysfs/basic-workstation/class/net"
iface "$BW/lo"    1 unknown "00:00:00:00:00:00" 65536 0x10049 772
stats  "$BW/lo"   1024 1024 16 16
iface "$BW/eth0"  2 up     "52:54:00:12:34:56" 1500  0x1043  1
stats  "$BW/eth0" 8123456 2345678 20011 18222
ln -sfn ../../../devices/pci0000:00/0000:00:03.0 "$BW/eth0/device"
iface "$BW/wlan0" 3 down   "a0:88:b4:22:33:44" 1500  0x1002  1
stats  "$BW/wlan0" 0 0 0 0
mkdir -p "$BW/wlan0/wireless"

# --- fixture: virtual-interfaces (original WO-01 data) ---------------------
VI="$ROOT/sysfs/virtual-interfaces/class/net"
iface "$VI/lo"     1  unknown "00:00:00:00:00:00" 65536 0x10049 772
stats  "$VI/lo"    512 512 8 8
iface "$VI/br0"    5  down    "ae:2f:55:aa:bb:cc" 1500  0x1002  1
stats  "$VI/br0"   0 0 0 0
mkdir -p "$VI/br0/bridge"
iface "$VI/wg0"    10 unknown ""                   1420  0x100c1 65534
iface "$VI/dummy0" 11 down    "66:2d:1a:55:aa:bb" 1500  0x10042 1
stats  "$VI/dummy0" 0 0 0 0

# --- fixture: procfs router-ish (original WO-01 data, masks corrected) ------
# Note: the surviving WO-01 script wrote the mask column big-endian
# (FFFFFF00); the kernel writes masks little-endian like every other value
# in /proc/net/route (00FFFFFF for /24, 000000FF for /8). Corrected during
# WO-01R reconstruction to match the real kernel format; the intended
# prefixes (lo /8, eth0 /24, wg0 /8) are preserved.
RT="$ROOT/procfs/router-ish/net"
write "$RT/route" "Iface        Destination     Gateway Flags   RefCnt  Use     Metric  Mask            MTU     Window  IRTT
eth0    00000000        0100A8C0        0003    0       0       600     00000000        0       0       0
eth0    0000A8C0        00000000        0001    0       0       0       00FFFFFF        0       0       0
lo      0000007F        00000000        0001    0       0       0       000000FF        0       0       0
wg0     00000A0A        00000000        0001    0       0       0       000000FF        0       0       0"
write "$RT/if_inet6" "00000000000000000000000000000001 01 80 10 80       lo
fe8000000000000020d530fffeac0034 03 40 20 80     eth0
20010db8000000000000000000000042 03 40 00 40     eth0"
# /proc/net/ipv6_route (WO-03 M4): format verified live (kernel 5.10,
# cross-checked with `ip -6 route`): dst dst_len src src_len nexthop metric
# refcnt use flags dev — hex except dev; no table column (rows span main
# and local; the model keeps table not-observed with a note).
write "$RT/ipv6_route" "00000000000000000000000000000000 00 00000000000000000000000000000000 00 00000000000000000000000000000000 ffffffff 00000001 00000000 00200200       lo
00000000000000000000000000000001 80 00000000000000000000000000000000 00 00000000000000000000000000000000 00000000 00000004 00000000 80200001       lo
fe800000000000000000000000000000 40 00000000000000000000000000000000 00 00000000000000000000000000000000 00000100 00000001 00000000 00000001     eth0
20010db8000000000000000000000000 40 00000000000000000000000000000000 00 00000000000000000000000000000000 00000100 00000001 00000000 00000001     eth0
20010db8000900000000000000000000 40 00000000000000000000000000000000 00 20010db8000000000000000000000002 00000400 00000001 00000000 00000003     eth0
ff000000000000000000000000000000 08 00000000000000000000000000000000 00 00000000000000000000000000000000 00000100 00000002 00000000 00000001     eth0"

# --- fixture: combined-laptop (WO-01R addition) ----------------------------
# A coherent sysfs+procfs pair: ifindexes agree between the trees, so a
# single full provider run produces a consistent observation.
CL="$ROOT/sysfs/combined-laptop/class/net"
iface "$CL/lo"    1 unknown "00:00:00:00:00:00" 65536 0x10049 772
stats  "$CL/lo"   256 256 4 4
iface "$CL/eth0"  2 up     "52:54:00:12:34:56" 1500  0x1043  1
stats  "$CL/eth0" 8123456 2345678 20011 18222
iface "$CL/wlan0" 3 down   "a0:88:b4:22:33:44" 1500  0x1002  1
stats  "$CL/wlan0" 0 0 0 0
mkdir -p "$CL/wlan0/wireless"
CLP="$ROOT/procfs/combined-laptop/net"
write "$CLP/route" "Iface        Destination     Gateway Flags   RefCnt  Use     Metric  Mask            MTU     Window  IRTT
eth0    00000000        0100A8C0        0003    0       0       600     00000000        0       0       0
eth0    0000A8C0        00000000        0001    0       0       0       00FFFFFF        0       0       0
lo      0000007F        00000000        0001    0       0       0       000000FF        0       0       0"
write "$CLP/if_inet6" "00000000000000000000000000000001 01 80 10 80       lo
fe80000000000000505400fffe123456 02 40 20 80     eth0
20010db80000000f0000000000000042 02 40 00 40     eth0"
write "$CLP/ipv6_route" "00000000000000000000000000000000 00 00000000000000000000000000000000 00 00000000000000000000000000000000 ffffffff 00000001 00000000 00200200       lo
00000000000000000000000000000001 80 00000000000000000000000000000000 00 00000000000000000000000000000000 00000000 00000004 00000000 80200001       lo
fe800000000000000000000000000000 40 00000000000000000000000000000000 00 00000000000000000000000000000000 00000100 00000001 00000000 00000001     eth0
20010db80000000f0000000000000000 40 00000000000000000000000000000000 00 00000000000000000000000000000000 00000100 00000001 00000000 00000001     eth0
ff000000000000000000000000000000 08 00000000000000000000000000000000 00 00000000000000000000000000000000 00000100 00000002 00000000 00000001     eth0"

# --- fixture: empty (WO-01R addition) --------------------------------------
# Zero interfaces, zero routes, zero addresses: a valid empty observation.
# (All three files exist and are empty — observed-and-empty, which is
# evidence, unlike a missing file, which is not.)
mkdir -p "$ROOT/sysfs/empty/class/net" "$ROOT/procfs/empty/net"
write "$ROOT/procfs/empty/net/route" "Iface        Destination     Gateway Flags   RefCnt  Use     Metric  Mask            MTU     Window  IRTT"
write "$ROOT/procfs/empty/net/if_inet6" ""
write "$ROOT/procfs/empty/net/ipv6_route" ""

echo "fixtures written under $ROOT"
