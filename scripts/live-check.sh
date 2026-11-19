#!/usr/bin/env bash
# Live read-only observation and cross-validation against iproute2, entirely
# inside a transient unprivileged network namespace (the host network is
# never modified; the namespace is destroyed on exit).
#
# Verifies the real binary end-to-end: classification, kernel-behavior
# semantics, provider states, JSON shape, agreement with `ip`, and the
# watch event stream.
set -euo pipefail
cd "$(dirname "$0")/.."
# shellcheck source=scripts/env.sh disable=SC1091
source scripts/env.sh

cargo build -q -p araiguma-cli
BIN="$CARGO_TARGET_DIR/debug/araiguma"

unshare -Urn bash -s "$BIN" <<'NETNS'
set -euo pipefail
BIN="$1"

# --- setup inside the isolated namespace -----------------------------------
ip link set lo up
ip link add ar-live0 type dummy
ip link set ar-live0 up
ip address add 192.0.2.10/24 dev ar-live0
ip address add fd00:d0:0:1::10/64 dev ar-live0 nodad
ip route add default via 192.0.2.1 dev ar-live0
# WO-03 live integration: a multipath route (M3) and an IPv6 gateway route
# (M4) observed by the real binary alongside everything else.
ip link add ar-live1 type dummy
ip link set ar-live1 up
ip address add 198.51.100.10/24 dev ar-live1
# ar-live0 needs an address in 198.51.100.0/24 for the multipath route's
# second-hop gateway to be on-link (the kernel rejects off-link nexthop
# gateways — discovery note 12).
ip address add 198.51.100.20/24 dev ar-live0
ip route add 203.0.114.0/24 nexthop via 198.51.100.1 dev ar-live1 weight 2 nexthop via 198.51.100.2 dev ar-live0 weight 3
ip route add 2001:db8:114::/64 via fd00:d0:0:1::1 dev ar-live0

fail() { echo "live-check: FAIL — $*" >&2; exit 1; }
check() { # check <description> <jq expression>
  if jq -e "$2" /tmp/live-snapshot.json > /dev/null; then
    echo "live-check: PASS — $1"
  else
    echo "live-check: FAIL — $1 (jq: $2)" >&2
    exit 1
  fi
}

# --- snapshot assertions -----------------------------------------------------
"$BIN" snapshot --pretty > /tmp/live-snapshot.json

check "dummy interface observed and classified by netlink" \
  '.interfaces | any(.name == "ar-live0" and .kind == "dummy" and .adminState == "up")'
check "operstate of an up dummy is unknown (kernel behavior, honestly reported)" \
  '.interfaces | any(.name == "ar-live0" and .operState == "unknown")'
check "IPv4 address observed with global scope" \
  '.addresses | any(.address == "192.0.2.10" and .prefixLen == 24 and .scope == "global")'
check "IPv6 address observed" \
  '.addresses | any(.address == "fd00:d0:0:1::10" and .prefixLen == 64)'
check "default route with gateway in the main table" \
  '.routes | any(.destination.prefixLen == 0 and .gateway == "192.0.2.1" and .table == 254 and .outputInterfaceName == "ar-live0")'
check "local table (255) present in the full dump" \
  '.routes | any(.table == 255 and .routeType == "local")'
check "netlink provider available" '.providers[0].state == "available"'
check "sysfs provider degraded (known fidelity limits)" '.providers[1].state == "degraded"'

# --- WO-03 feature checks (M3 multipath, M4 IPv6 procfs) ----------------------
check "multipath route carries its FULL next-hop set (WO-03 M3)" \
  '.routes | any(.destination.address == "203.0.114.0" and .nextHops.multi and (.nextHops.multi | length == 2) and ((.nextHops.multi | map(.weight) | sort) == [2, 3]))'
check "multipath route-level gateway stays null (hops carry them)" \
  '.routes | any(.destination.address == "203.0.114.0" and .gateway == null and .outputInterface == null)'
check "multipath route united with the procfs first-hop view" \
  '.routes | any(.destination.address == "203.0.114.0" and (.sources | index("procfs") != null))'
check "IPv6 gateway route observed once with united provenance (WO-03 M4)" \
  '[.routes[] | select(.destination.address == "2001:db8:114::")] | length == 1 and (.[] | .gateway == "fd00:d0:0:1::1") and ((.[] | .sources) | index("procfs") != null)'
check "IPv6 local-table ::1/128 deduplicated against netlink by the table-less wildcard" \
  '[.routes[] | select(.destination.address == "::1")] | length == 1 and (.[] | .table == 255)'
check "the kernel's IPv6 unreachable-default sentinel is kept (procfs-only, table not observed)" \
  '.routes | any(.family == "ipv6" and .destination.prefixLen == 0 and .metric == 4294967295 and .table == null)'
check "sysfs routesV6 section is observed, not unsupported (WO-03 M4)" \
  '.providers[1].sections[] | select(.section == "routesV6") | .outcome.kind == "observed"'

# --- cross-validation against iproute2 in the same namespace -----------------
ip_gw="$(ip -4 -j route | jq -r '.[] | select(.dst == "default") | .gateway')"
snap_gw="$(jq -r '.routes[] | select(.family == "ipv4" and .destination.prefixLen == 0) | .gateway' /tmp/live-snapshot.json)"
[ "$ip_gw" = "$snap_gw" ] || fail "default gateway mismatch: iproute2 says '$ip_gw', snapshot says '$snap_gw'"
echo "live-check: PASS — default gateway agrees with iproute2 ($ip_gw)"

ip_addr="$(ip -4 -j addr show dev ar-live0 | jq -r '.[0].addr_info[0].local')"
jq -e --arg addr "$ip_addr" '.addresses | any(.address == $addr)' /tmp/live-snapshot.json > /dev/null \
  || fail "address $ip_addr missing from snapshot"
echo "live-check: PASS — address $ip_addr agrees with iproute2"

# WO-03 cross-validation: the multipath hop set and the v6 route.
ip_mp="$(ip -j route show 203.0.114.0/24 | jq -r '.[0].nexthops | map(.weight) | join("/")')"
[ "$ip_mp" = "2/3" ] || fail "iproute2 multipath weights unexpected: '$ip_mp'"
echo "live-check: PASS — multipath weights agree with iproute2 (2/3)"

ip_v6gw="$(ip -6 -j route show 2001:db8:114::/64 | jq -r '.[0].gateway')"
snap_v6gw="$(jq -r '.routes[] | select(.destination.address == "2001:db8:114::") | .gateway' /tmp/live-snapshot.json)"
[ "$ip_v6gw" = "$snap_v6gw" ] || fail "v6 gateway mismatch: iproute2 says '$ip_v6gw', snapshot says '$snap_v6gw'"
echo "live-check: PASS — IPv6 gateway agrees with iproute2 ($ip_v6gw)"

# --- watch smoke --------------------------------------------------------------
(timeout 10 "$BIN" watch > /tmp/live-watch.out 2> /tmp/live-watch.err) &
watch_pid=$!
sleep 1.5
ip link add ar-live2 type dummy
sleep 1.5
ip link del ar-live2
wait "$watch_pid" 2>/dev/null || true

grep -q '"event":"snapshot"' /tmp/live-watch.out || fail "watch produced no initial snapshot event"
changed_lines="$(grep -c '"event":"changed"' /tmp/live-watch.out || true)"
[ "$changed_lines" -ge 2 ] || fail "watch produced only $changed_lines change events (expected >= 2)"
[ -s /tmp/live-watch.err ] && { echo "--- watch stderr ---"; cat /tmp/live-watch.err; }
echo "live-check: PASS — watch emitted a snapshot event and $changed_lines change events"

# --- documents for the frontend import integration (WO-03 M1) ---------------
# A real CLI-produced snapshot plus its projected topology document, left
# for the gate's frontend step to parse with the actual frontend
# validators (see tests/live-document.test.ts and scripts/test-all.sh).
# The directory is overridable so the fresh-clone validation (WO-03-FC)
# keeps its live documents isolated from the primary checkout's.
LIVE_DOCS_DIR="${ARIGUMA_LIVE_DOCS_DIR:-/tmp/araiguma-live-docs}"
rm -rf "$LIVE_DOCS_DIR"
mkdir -p "$LIVE_DOCS_DIR"
"$BIN" snapshot > "$LIVE_DOCS_DIR/live-snapshot.json" \
  || fail "snapshot export for the frontend failed"
"$BIN" topology --input "$LIVE_DOCS_DIR/live-snapshot.json" \
  > "$LIVE_DOCS_DIR/live-topology.json" \
  || fail "topology projection for the frontend failed"

echo "live-check: PASS — CLI snapshot + topology documents exported for frontend validation"
echo "live-check: ALL LIVE CHECKS PASSED (inside a transient network namespace)"
NETNS
