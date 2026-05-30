#!/bin/bash
# Build libhev-socks5-tunnel.so for Android (arm64-v8a, armeabi-v7a, x86_64)
#
# Prerequisites:
#   1. Android NDK installed (e.g., ~/Android/Sdk/ndk/27.0.12077973)
#   2. git
#   3. make
#
# Usage:
#   export ANDROID_NDK_HOME=$HOME/Android/Sdk/ndk/27.0.12077973
#   ./scripts/build-tun2socks-android.sh

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
OUTPUT_DIR="$PROJECT_ROOT/ghost_flutter/android/app/src/main/jniLibs"
BUILD_DIR="$PROJECT_ROOT/.build/tun2socks"

NDK="${ANDROID_NDK_HOME:-${ANDROID_NDK}}"
if [ -z "$NDK" ]; then
    echo "ERROR: ANDROID_NDK_HOME or ANDROID_NDK is not set"
    echo "Example: export ANDROID_NDK_HOME=\$HOME/Android/Sdk/ndk/27.0.12077973"
    exit 1
fi

# Clone hev-socks5-tunnel if not exists
REPO_URL="https://github.com/heiher/hev-socks5-tunnel.git"
REPO_DIR="$BUILD_DIR/hev-socks5-tunnel"

if [ ! -d "$REPO_DIR" ]; then
    echo "Cloning hev-socks5-tunnel..."
    mkdir -p "$BUILD_DIR"
    git clone --depth=1 "$REPO_URL" "$REPO_DIR"
else
    echo "Using existing hev-socks5-tunnel at $REPO_DIR"
fi

cd "$REPO_DIR"

# The project builds with ndk-build or CMake depending on version.
# Recent versions use CMake + Android NDK toolchain.
# We try CMake first, fallback to ndk-build.

build_abi() {
    local ABI=$1
    local ARCH=$2
    local OUTPUT_SUBDIR=$3

    echo ""
    echo "=== Building for $ABI ($ARCH) ==="

    local ABI_BUILD_DIR="$BUILD_DIR/build-$ABI"
    mkdir -p "$ABI_BUILD_DIR"
    cd "$ABI_BUILD_DIR"

    cmake "$REPO_DIR" \
        -DCMAKE_TOOLCHAIN_FILE="$NDK/build/cmake/android.toolchain.cmake" \
        -DANDROID_ABI="$ABI" \
        -DANDROID_PLATFORM=android-21 \
        -DCMAKE_BUILD_TYPE=Release \
        -DBUILD_STATIC_LIBS=OFF \
        -DBUILD_SHARED_LIBS=ON

    cmake --build . --parallel "$(nproc)"

    # Find the built .so
    local SO_FILE
    SO_FILE=$(find . -name "libhev-socks5-tunnel.so" | head -n1)

    if [ -z "$SO_FILE" ]; then
        echo "WARNING: libhev-socks5-tunnel.so not found for $ABI"
        return 1
    fi

    mkdir -p "$OUTPUT_DIR/$OUTPUT_SUBDIR"
    cp "$SO_FILE" "$OUTPUT_DIR/$OUTPUT_SUBDIR/libhev-socks5-tunnel.so"
    echo "Copied to $OUTPUT_DIR/$OUTPUT_SUBDIR/libhev-socks5-tunnel.so"
}

# Try CMake build for each ABI
build_abi "arm64-v8a"   "aarch64" "arm64-v8a"  || true
build_abi "armeabi-v7a" "arm"     "armeabi-v7a" || true
build_abi "x86_64"      "x86_64"  "x86_64"     || true

echo ""
echo "=== Build complete ==="
echo "Output: $OUTPUT_DIR"
find "$OUTPUT_DIR" -name "*.so" -exec ls -lh {} \;
