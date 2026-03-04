#!/usr/bin/env bash
set -euo pipefail

if [ "$#" -ne 3 ]; then
    echo "Usage: $0 <version-without-v> <manifest-path> <checksums-file>" >&2
    exit 1
fi

version="$1"
manifest_path="$2"
checksums_path="$3"
repo_slug="${REPO_SLUG:-jwcraig/sql-server-cli}"
artifact_name="sscli-x86_64-pc-windows-msvc.zip"

if [ ! -f "$manifest_path" ]; then
    echo "Scoop manifest not found: $manifest_path" >&2
    exit 1
fi

if [ ! -f "$checksums_path" ]; then
    echo "Checksums file not found: $checksums_path" >&2
    exit 1
fi

if ! command -v jq > /dev/null 2>&1; then
    echo "jq is required to update the scoop manifest." >&2
    exit 1
fi

windows_sha="$(awk -v wanted="$artifact_name" '$2 == wanted { print $1 }' "$checksums_path" | head -n 1)"
if [ -z "$windows_sha" ]; then
    echo "Missing checksum for $artifact_name in $checksums_path" >&2
    exit 1
fi

expected_url="https://github.com/${repo_slug}/releases/download/v${version}/${artifact_name}"

tmp_manifest="${manifest_path}.tmp"
jq \
    --arg version "$version" \
    --arg sha256 "$windows_sha" \
    --arg url "$expected_url" \
    '.version = $version
     | .architecture."64bit".url = $url
     | .architecture."64bit".hash = $sha256' \
    "$manifest_path" > "$tmp_manifest"

mv "$tmp_manifest" "$manifest_path"

manifest_url="$(jq -r '.architecture."64bit".url' "$manifest_path")"
manifest_hash="$(jq -r '.architecture."64bit".hash' "$manifest_path")"

if [ "$manifest_url" != "$expected_url" ]; then
    echo "Scoop manifest URL mismatch: expected $expected_url, got $manifest_url" >&2
    exit 1
fi

if [ "$manifest_hash" != "$windows_sha" ]; then
    echo "Scoop manifest hash mismatch: expected $windows_sha, got $manifest_hash" >&2
    exit 1
fi

tmp_dir="$(mktemp -d)"
cleanup() {
    rm -rf "$tmp_dir"
}
trap cleanup EXIT

curl -fsSL "$manifest_url" -o "$tmp_dir/$artifact_name"
actual_sha="$(sha256sum "$tmp_dir/$artifact_name" | awk '{print $1}')"

if [ "$actual_sha" != "$manifest_hash" ]; then
    echo "Downloaded scoop artifact checksum mismatch for $artifact_name" >&2
    echo "expected=$manifest_hash actual=$actual_sha" >&2
    exit 1
fi

echo "Scoop manifest updated and verified: $manifest_path"
