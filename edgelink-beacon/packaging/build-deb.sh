#!/usr/bin/env bash
set -euo pipefail

project_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
repo_root="$(cd "$project_dir/../.." && pwd)"
target=""

usage() {
    cat <<'EOF'
Build a Debian package for edgelink-beacon.

Usage: packaging/build-deb.sh [--target RUST_TARGET]

Builds natively for the current Debian system by default. For a cross build,
pass a Rust target triple and install the matching Rust target/toolchain first.
EOF
}

while (($#)); do
    case "$1" in
        --target)
            [[ $# -ge 2 ]] || { echo "--target requires a Rust target triple" >&2; exit 2; }
            target="$2"
            shift 2
            ;;
        --help|-h)
            usage
            exit 0
            ;;
        *)
            echo "Unknown argument: $1" >&2
            usage >&2
            exit 2
            ;;
    esac
done

command -v cargo >/dev/null || { echo "cargo is required" >&2; exit 1; }
command -v dpkg-deb >/dev/null || { echo "dpkg-deb is required (install dpkg-dev)" >&2; exit 1; }

version="$(sed -n 's/^version = "\([^"]*\)"/\1/p' "$project_dir/Cargo.toml" | head -n 1)"
[[ -n "$version" ]] || { echo "Could not read package version from Cargo.toml" >&2; exit 1; }

build_args=(--release --manifest-path "$project_dir/Cargo.toml")
if [[ -n "$target" ]]; then
    build_args+=(--target "$target")
    rust_arch="${target%%-*}"
else
    rust_target="$(rustc -vV | sed -n 's/^host: //p')"
    rust_arch="${rust_target%%-*}"
fi

case "$rust_arch" in
    aarch64) deb_arch=arm64 ;;
    armv7*|armv7-*) deb_arch=armhf ;;
    x86_64) deb_arch=amd64 ;;
    i686|i586) deb_arch=i386 ;;
    *)
        echo "Unsupported Debian architecture mapping for Rust target: $rust_arch" >&2
        exit 1
        ;;
esac

cargo build "${build_args[@]}"
if [[ -n "$target" ]]; then
    binary="$project_dir/target/$target/release/edgelink-beacon"
else
    binary="$project_dir/target/release/edgelink-beacon"
fi
[[ -x "$binary" ]] || { echo "Built executable not found: $binary" >&2; exit 1; }

package_name="edgelink-beacon_${version}_${deb_arch}.deb"
output_dir="$repo_root/dist"
mkdir -p "$output_dir"
stage_dir="$(mktemp -d)"
trap 'rm -rf "$stage_dir"' EXIT

install -D -m 0755 "$binary" "$stage_dir/usr/bin/edgelink-beacon"
install -D -m 0644 "$project_dir/packaging/edgelink-beacon.service" \
    "$stage_dir/lib/systemd/system/edgelink-beacon.service"
mkdir -p "$stage_dir/usr/share/doc/edgelink-beacon"
cat > "$stage_dir/usr/share/doc/edgelink-beacon/README.Debian" <<'EOF'
The service listens on UDP port 40191 by default. Allow inbound UDP/40191 in
the host firewall when one is enabled. The systemd unit runs with DynamicUser.
EOF

mkdir -p "$stage_dir/DEBIAN"
cat > "$stage_dir/DEBIAN/control" <<EOF
Package: edgelink-beacon
Version: $version
Section: net
Priority: optional
Architecture: $deb_arch
Depends: systemd, init-system-helpers
Maintainer: EdgeLink Project <noreply@localhost>
Description: EdgeLink Beacon - UDP discovery responder for Edge Linux boards
 Lets EdgeLink Router Windows clients discover the board's current IPv4 address
 over the local network without requiring a fixed address on the board.
EOF

cat > "$stage_dir/DEBIAN/postinst" <<'EOF'
#!/bin/sh
set -e
if [ "$1" = configure ]; then
    if command -v deb-systemd-helper >/dev/null 2>&1; then
        deb-systemd-helper unmask edgelink-beacon.service >/dev/null || true
        deb-systemd-helper enable edgelink-beacon.service >/dev/null || true
    fi
    if command -v deb-systemd-invoke >/dev/null 2>&1; then
        deb-systemd-invoke start edgelink-beacon.service >/dev/null || true
    elif command -v systemctl >/dev/null 2>&1; then
        systemctl enable --now edgelink-beacon.service >/dev/null || true
    fi
fi
exit 0
EOF

cat > "$stage_dir/DEBIAN/prerm" <<'EOF'
#!/bin/sh
set -e
if [ "$1" = remove ] || [ "$1" = deconfigure ]; then
    if command -v deb-systemd-invoke >/dev/null 2>&1; then
        deb-systemd-invoke stop edgelink-beacon.service >/dev/null || true
    elif command -v systemctl >/dev/null 2>&1; then
        systemctl stop edgelink-beacon.service >/dev/null || true
    fi
    if command -v deb-systemd-helper >/dev/null 2>&1; then
        deb-systemd-helper disable edgelink-beacon.service >/dev/null || true
    fi
fi
exit 0
EOF

cat > "$stage_dir/DEBIAN/postrm" <<'EOF'
#!/bin/sh
set -e
if [ "$1" = remove ] || [ "$1" = purge ] || [ "$1" = disappear ]; then
    if command -v deb-systemd-helper >/dev/null 2>&1; then
        deb-systemd-helper purge edgelink-beacon.service >/dev/null || true
        deb-systemd-helper unmask edgelink-beacon.service >/dev/null || true
    fi
    if command -v systemctl >/dev/null 2>&1; then
        systemctl daemon-reload >/dev/null || true
    fi
fi
exit 0
EOF
chmod 0755 "$stage_dir/DEBIAN/postinst" "$stage_dir/DEBIAN/prerm" "$stage_dir/DEBIAN/postrm"

dpkg-deb --root-owner-group --build "$stage_dir" "$output_dir/$package_name"
echo "Created $output_dir/$package_name"
