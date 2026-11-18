#!/usr/bin/env bash
# Capability check: can this environment run a PRIVATE D-Bus session bus?
#
# Araiguma does not depend on D-Bus in WO-01R/WO-02. This probe records
# whether the foundation for future NetworkManager / systemd-resolved
# integration tests exists: a private session bus started with
# dbus-run-session, answering a basic query. It never touches a system bus.
#
# Outcome is recorded, not enforced: this script exits 0 unless the check
# itself could not run, because "capability absent" is a valid finding.
set -uo pipefail
cd "$(dirname "$0")/.."

if ! command -v dbus-run-session >/dev/null 2>&1; then
  echo "dbus-harness: UNAVAILABLE (dbus-run-session not installed)"
  exit 0
fi
if ! command -v dbus-send >/dev/null 2>&1; then
  echo "dbus-harness: LIMITED (dbus-run-session present, dbus-send missing)"
  exit 0
fi

out="$(timeout 15 dbus-run-session -- \
  dbus-send --session --print-reply --dest=org.freedesktop.DBus \
  /org/freedesktop/DBus org.freedesktop.DBus.ListNames 2>&1 || true)"

if printf '%s\n' "$out" | sed -n '1p' | grep -q 'method return'; then
  echo "dbus-harness: PASS (private session bus started; ListNames answered)"
  exit 0
fi

echo "dbus-harness: UNEXPECTED-FAILURE (dbus-run-session present but session bus failed)"
printf '%s\n' "$out" | sed -n '1,5p'
exit 0
