#!/bin/sh
# Cross-build for Blackbird OS on aarch64 from a developer host.
# Usage: scripts/cross.sh <cargo subcommand args...>     e.g. scripts/cross.sh build --release -p dap-shell
#
# Links with zig (cargo-zigbuild) against the image's own glibc and libraries: the sysroot that
# blackbird-os/build/dap-sysroot.sh builds from the same Debian packages as the image.
set -eu
here="$(cd "$(dirname "$0")/.." && pwd)"
sysroot="${OPENDAP_SYSROOT_DIR:-${XDG_CACHE_HOME:-$HOME/.cache}/opendap/sysroot/trixie-arm64}"
# glibc of the image (Debian trixie: libc6 2.41); zig links against exactly this version.
glibc="${OPENDAP_GLIBC:-2.41}"
target=aarch64-unknown-linux-gnu

if [ ! -d "$sysroot/usr/lib/$target" ] && [ ! -d "$sysroot/usr/lib/aarch64-linux-gnu" ]; then
  echo "no sysroot at $sysroot: run blackbird-os/build/dap-sysroot.sh" >&2
  exit 1
fi
lib="$sysroot/usr/lib/aarch64-linux-gnu"

export PKG_CONFIG_ALLOW_CROSS=1
export PKG_CONFIG_SYSROOT_DIR="$sysroot"
export PKG_CONFIG_LIBDIR="$lib/pkgconfig:$sysroot/usr/share/pkgconfig"
export PKG_CONFIG_PATH=""
# Link against the sysroot's libraries; glibc itself comes from zig's stubs for that version.
export CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_RUSTFLAGS="-L native=$lib ${RUSTFLAGS:-}"

cmd="$1"; shift
cd "$here"
exec cargo zig"$cmd" --target "$target.$glibc" "$@"
