#!/bin/sh
# Deploy system binaries to a running Blackbird OS instance as a development system extension.
# Usage: scripts/deploy.sh [--debug] [crate…]      default crates: dap-shell dap-product dap-inject
#
# Cross-builds on this host (scripts/cross.sh), packs the binaries as the system extension
# `opendap-dev` (/var/lib/extensions/opendap-dev: usr/bin/… plus its extension-release file),
# merges it with `systemd-sysext refresh` and restarts the shell. The image is not rebuilt.
# `systemd-sysext unmerge` and removing the directory return the instance to its image.
#
# Target: the running virtual device through virtual-devices/build/dap-qemu.sh ssh (VIRTUAL_DEVICES
# points at that checkout; default: next to this one). Until the `opendap` CLI carries this.
set -eu
here="$(cd "$(dirname "$0")/.." && pwd)"
remote="${VIRTUAL_DEVICES:-$here/../virtual-devices}/build/dap-qemu.sh"
profile=release
flag=--release
if [ "${1:-}" = --debug ]; then profile=debug; flag=; shift; fi
crates="${*:-dap-shell dap-product dap-inject}"

pkgs=""
for c in $crates; do pkgs="$pkgs -p $c"; done
# shellcheck disable=SC2086
"$here/scripts/cross.sh" build $flag $pkgs

stage="$(mktemp -d)"
trap 'rm -rf "$stage"' EXIT
mkdir -p "$stage/usr/bin" "$stage/usr/lib/extension-release.d"
for c in $crates; do cp "$here/target/aarch64-unknown-linux-gnu/$profile/$c" "$stage/usr/bin/"; done
# ID=_any: a development extension applies to whichever Blackbird OS image the instance runs.
printf 'ID=_any\nARCHITECTURE=arm64\n' > "$stage/usr/lib/extension-release.d/extension-release.opendap-dev"

# No macOS metadata in the archive; the instance keeps its own clock for file times.
COPYFILE_DISABLE=1 tar --no-mac-metadata --no-xattrs -C "$stage" -cf - usr | "$remote" ssh 'set -e
  d=/var/lib/extensions/opendap-dev
  rm -rf "$d.new" && mkdir -p "$d.new" && tar -C "$d.new" -m --no-same-owner -xf -
  systemd-sysext unmerge >/dev/null 2>&1 || true
  rm -rf "$d" && mv "$d.new" "$d"
  sync
  systemd-sysext refresh
  systemctl restart dap-shell.service
  systemctl --no-pager --lines=0 status dap-shell.service | head -3'
