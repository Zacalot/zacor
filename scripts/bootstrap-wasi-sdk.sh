#!/usr/bin/env sh
set -eu

version="32.0"
release="wasi-sdk-32"

os_name=$(uname -s)
arch_name=$(uname -m)

case "$os_name" in
  Linux) host_os="linux" ;;
  Darwin) host_os="macos" ;;
  *)
    echo "unsupported host OS: $os_name" >&2
    exit 1
    ;;
esac

case "$arch_name" in
  x86_64|amd64) host_arch="x86_64" ;;
  arm64|aarch64) host_arch="arm64" ;;
  *)
    echo "unsupported host arch: $arch_name" >&2
    exit 1
    ;;
esac

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repo_root=$(CDPATH= cd -- "$script_dir/.." && pwd)
archive_name="wasi-sdk-$version-$host_arch-$host_os.tar.gz"
url="https://github.com/WebAssembly/wasi-sdk/releases/download/$release/$archive_name"
archive="$repo_root/wasi-sdk.tar.gz"
sdk_dir="$repo_root/wasi-sdk"
extracted="$repo_root/wasi-sdk-$version-$host_arch-$host_os"

rm -rf "$sdk_dir"

if command -v curl >/dev/null 2>&1; then
  curl -L "$url" -o "$archive"
elif command -v wget >/dev/null 2>&1; then
  wget -O "$archive" "$url"
else
  echo "missing downloader: install curl or wget" >&2
  exit 1
fi

tar -xzf "$archive" -C "$repo_root"
rm -f "$archive"
mv "$extracted" "$sdk_dir"
