#!/usr/bin/env bash
set -euo pipefail

if [ "$#" -ne 3 ]; then
    echo "Usage: $0 <version-without-v> <formula-path> <checksums-file>" >&2
    exit 1
fi

version="$1"
formula_path="$2"
checksums_path="$3"
repo_slug="${REPO_SLUG:-jwcraig/sql-server-cli}"

if [ ! -f "$formula_path" ]; then
    echo "Formula file not found: $formula_path" >&2
    exit 1
fi

if [ ! -f "$checksums_path" ]; then
    echo "Checksums file not found: $checksums_path" >&2
    exit 1
fi

sha_for_artifact() {
    local artifact="$1"
    awk -v wanted="$artifact" '$2 == wanted { print $1 }' "$checksums_path" | head -n 1
}

sha_macos_x86_64="$(sha_for_artifact "sscli-x86_64-apple-darwin.tar.gz")"
sha_macos_aarch64="$(sha_for_artifact "sscli-aarch64-apple-darwin.tar.gz")"
sha_linux_x86_64="$(sha_for_artifact "sscli-x86_64-unknown-linux-gnu.tar.gz")"
sha_linux_aarch64="$(sha_for_artifact "sscli-aarch64-unknown-linux-gnu.tar.gz")"

for value_name in sha_macos_x86_64 sha_macos_aarch64 sha_linux_x86_64 sha_linux_aarch64; do
    if [ -z "${!value_name}" ]; then
        echo "Missing checksum value: $value_name" >&2
        echo "Expected checksums in: $checksums_path" >&2
        exit 1
    fi
done

export VERSION="$version"
export FORMULA_PATH="$formula_path"
export REPO_SLUG="$repo_slug"
export SHA_MACOS_X86_64="$sha_macos_x86_64"
export SHA_MACOS_AARCH64="$sha_macos_aarch64"
export SHA_LINUX_X86_64="$sha_linux_x86_64"
export SHA_LINUX_AARCH64="$sha_linux_aarch64"

ruby <<'RUBY'
path = ENV.fetch("FORMULA_PATH")
version = ENV.fetch("VERSION")
repo_slug = ENV.fetch("REPO_SLUG")

checksums = {
  "sscli-x86_64-apple-darwin.tar.gz" => ENV.fetch("SHA_MACOS_X86_64"),
  "sscli-aarch64-apple-darwin.tar.gz" => ENV.fetch("SHA_MACOS_AARCH64"),
  "sscli-x86_64-unknown-linux-gnu.tar.gz" => ENV.fetch("SHA_LINUX_X86_64"),
  "sscli-aarch64-unknown-linux-gnu.tar.gz" => ENV.fetch("SHA_LINUX_AARCH64")
}

lines = File.readlines(path, chomp: false)

lines.each_with_index do |line, index|
  if line.match?(/^  version "/)
    lines[index] = "  version \"#{version}\"\n"
    next
  end

  match = line.match(%r{^(\s*)url "https://github\.com/[^/]+/[^/]+/releases/download/v[^/]+/(sscli-[^"]+)"})
  next unless match

  indent = match[1]
  artifact = match[2]
  next unless checksums.key?(artifact)

  lines[index] = "#{indent}url \"https://github.com/#{repo_slug}/releases/download/v#{version}/#{artifact}\"\n"

  sha_index = index + 1
  sha_line = lines[sha_index]
  unless sha_line&.match?(/^\s*sha256 "/)
    raise "Expected sha256 line after url for #{artifact} in #{path}"
  end

  lines[sha_index] = sha_line.sub(/sha256 "[^"]+"/, "sha256 \"#{checksums.fetch(artifact)}\"")
end

File.write(path, lines.join)
RUBY

tmp_dir="$(mktemp -d)"
cleanup() {
    rm -rf "$tmp_dir"
}
trap cleanup EXIT

verify_artifact() {
    local artifact="$1"
    local expected_sha="$2"
    local formula_url="https://github.com/${repo_slug}/releases/download/v${version}/${artifact}"

    if ! grep -F "url \"${formula_url}\"" "$formula_path" > /dev/null; then
        echo "Formula URL mismatch for ${artifact}" >&2
        exit 1
    fi

    if ! grep -F "sha256 \"${expected_sha}\"" "$formula_path" > /dev/null; then
        echo "Formula SHA256 mismatch for ${artifact}" >&2
        exit 1
    fi

    curl -fsSL "$formula_url" -o "$tmp_dir/$artifact"
    actual_sha="$(sha256sum "$tmp_dir/$artifact" | awk '{print $1}')"

    if [ "$actual_sha" != "$expected_sha" ]; then
        echo "Downloaded artifact checksum mismatch for ${artifact}" >&2
        echo "expected=${expected_sha} actual=${actual_sha}" >&2
        exit 1
    fi
}

verify_artifact "sscli-x86_64-apple-darwin.tar.gz" "$sha_macos_x86_64"
verify_artifact "sscli-aarch64-apple-darwin.tar.gz" "$sha_macos_aarch64"
verify_artifact "sscli-x86_64-unknown-linux-gnu.tar.gz" "$sha_linux_x86_64"
verify_artifact "sscli-aarch64-unknown-linux-gnu.tar.gz" "$sha_linux_aarch64"

echo "Homebrew formula updated and verified: $formula_path"
