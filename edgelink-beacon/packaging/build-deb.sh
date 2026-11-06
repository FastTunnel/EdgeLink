#!/usr/bin/env bash
set -euo pipefail

project_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
repo_root="$(cd "$project_dir/.." && pwd)"
target=""
build_all=false
use_musl=true

usage() {
    cat <<'EOF'
Build Debian packages (.deb) for edgelink-beacon.

Usage: packaging/build-deb.sh [OPTIONS]

Options:
  --arch arm64|x86_64     Build for specific architecture (defaults to musl static)
  --musl                  Use musl libc for 100% static binary (default: true)
  --gnu                   Use gnu glibc instead of musl
  --target RUST_TARGET    Pass explicit Rust target triple
  --all                   Build both x86_64 and aarch64 (ARM64) packages
  -h, --help              Show this help message

Examples:
  packaging/build-deb.sh --arch arm64       # Static ARM64 (musl) for RK3588, Raspberry Pi, etc.
  packaging/build-deb.sh --arch x86_64      # Static x86_64 (musl) for general Linux
  packaging/build-deb.sh --all              # Build both x86_64 and arm64 musl packages
EOF
}

while (($#)); do
    case "$1" in
        --target)
            [[ $# -ge 2 ]] || { echo "--target requires a Rust target triple" >&2; exit 2; }
            target="$2"
            shift 2
            ;;
        --arch)
            [[ $# -ge 2 ]] || { echo "--arch requires an architecture (arm64, x86_64, amd64, aarch64)" >&2; exit 2; }
            case "$2" in
                arm64|aarch64)
                    if [ "$use_musl" = true ]; then
                        target="aarch64-unknown-linux-musl"
                    else
                        target="aarch64-unknown-linux-gnu"
                    fi
                    ;;
                x86_64|amd64)
                    if [ "$use_musl" = true ]; then
                        target="x86_64-unknown-linux-musl"
                    else
                        target="x86_64-unknown-linux-gnu"
                    fi
                    ;;
                *) echo "Unknown arch: $2 (use arm64 or x86_64)" >&2; exit 2 ;;
            esac
            shift 2
            ;;
        --musl)
            use_musl=true
            shift
            ;;
        --gnu)
            use_musl=false
            shift
            ;;
        --all)
            build_all=true
            shift
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

build_single_target() {
    local cur_target="$1"
    local version
    version="$(sed -n 's/^version = "\([^"]*\)"/\1/p' "$project_dir/Cargo.toml" | head -n 1)"
    [[ -n "$version" ]] || { echo "Could not read package version from Cargo.toml" >&2; exit 1; }

    local rust_arch deb_arch

    if [[ -n "$cur_target" ]]; then
        rust_arch="${cur_target%%-*}"
    else
        local rust_target
        rust_target="$(rustc -vV | sed -n 's/^host: //p')"
        rust_arch="${rust_target%%-*}"
        cur_target="$rust_target"
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

    local builder="cargo"
    if [[ "$cur_target" == *"musl"* && "$rust_arch" == "aarch64" ]]; then
        if command -v cross >/dev/null 2>&1; then
            builder="cross"
        elif command -v aarch64-linux-musl-gcc >/dev/null 2>&1; then
            export CARGO_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_LINKER="aarch64-linux-musl-gcc"
        elif command -v aarch64-linux-gnu-gcc >/dev/null 2>&1; then
            export CARGO_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_LINKER="aarch64-linux-gnu-gcc"
        fi
    elif [[ "$cur_target" == *"musl"* && "$rust_arch" == "x86_64" ]]; then
        if command -v musl-gcc >/dev/null 2>&1; then
            export CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER="musl-gcc"
        fi
    fi

    echo "==> Building edgelink-beacon using [$builder] for target: $cur_target (Debian arch: $deb_arch)..."
    $builder build --release --manifest-path "$project_dir/Cargo.toml" --target "$cur_target"

    local binary="$project_dir/target/$cur_target/release/edgelink-beacon"
    [[ -x "$binary" ]] || { echo "Built executable not found: $binary" >&2; exit 1; }

    local package_name="edgelink-beacon_${version}_${deb_arch}.deb"
    local output_dir="$repo_root/dist"
    mkdir -p "$output_dir"
    local stage_dir
    stage_dir="$(mktemp -d)"
    trap 'rm -rf "$stage_dir"' RETURN

    install -D -m 0755 "$binary" "$stage_dir/usr/bin/edgelink-beacon"
    install -D -m 0644 "$project_dir/packaging/edgelink-beacon.service" \
        "$stage_dir/lib/systemd/system/edgelink-beacon.service"
    mkdir -p "$stage_dir/usr/share/doc/edgelink-beacon"
    cat > "$stage_dir/usr/share/doc/edgelink-beacon/README.Debian" <<'EOF'
The service listens on UDP port 40191 by default. Statically linked with musl libc.
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
Description: EdgeLink Beacon - Musl pure static UDP discovery responder for Edge Linux boards
 100% statically linked with musl libc. Guaranteed to run on any Linux distribution/kernel
 without glibc version mismatch issues.
EOF

    cat > "$stage_dir/DEBIAN/postinst" <<'EOF'
#!/bin/sh
set -e
if [ "$1" = "configure" ]; then
    systemctl --system daemon-reload >/dev/null 2>&1 || true
    systemctl enable edgelink-beacon.service >/dev/null 2>&1 || true
    systemctl restart edgelink-beacon.service >/dev/null 2>&1 || true
fi
exit 0
EOF
    chmod 0755 "$stage_dir/DEBIAN/postinst"

    cat > "$stage_dir/DEBIAN/prerm" <<'EOF'
#!/bin/sh
set -e
if [ "$1" = "remove" ] || [ "$1" = "deconfigure" ]; then
    systemctl stop edgelink-beacon.service >/dev/null 2>&1 || true
    systemctl disable edgelink-beacon.service >/dev/null 2>&1 || true
fi
exit 0
EOF
    chmod 0755 "$stage_dir/DEBIAN/prerm"

    cat > "$stage_dir/DEBIAN/postrm" <<'EOF'
#!/bin/sh
set -e
if [ "$1" = "purge" ] || [ "$1" = "remove" ]; then
    systemctl --system daemon-reload >/dev/null 2>&1 || true
fi
exit 0
EOF
    chmod 0755 "$stage_dir/DEBIAN/postrm"

    dpkg-deb --build --root-owner-group "$stage_dir" "$output_dir/$package_name"
    echo "==> Successfully created package: $output_dir/$package_name"
}

if [ "$build_all" = true ]; then
    if [ "$use_musl" = true ]; then
        build_single_target "x86_64-unknown-linux-musl"
        build_single_target "aarch64-unknown-linux-musl"
    else
        build_single_target "x86_64-unknown-linux-gnu"
        build_single_target "aarch64-unknown-linux-gnu"
    fi
else
    build_single_target "$target"
fi
