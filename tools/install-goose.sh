#!/usr/bin/env bash
set -euo pipefail
cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.."
if [[ "$(uname -s)" != Linux || "$(uname -m)" != x86_64 ]]; then
  echo 'This pinned local proof currently supports Linux x86_64 only.' >&2
  exit 1
fi
version=1.54.0
checksum=ab396c95aebbd28b4d38c175234be8d4a3ea0c96e7ccb24c307e4618c9be57e1
install_dir=".cache/goose/v${version}"
mkdir -p -- "$install_dir"
archive=$(mktemp "$install_dir/download.XXXXXX")
trap 'rm -f -- "$archive"' EXIT
curl --fail --location --retry 2 --output "$archive" \
  "https://github.com/aaif-goose/goose/releases/download/v${version}/goose-x86_64-unknown-linux-gnu.tar.gz"
printf '%s  %s\n' "$checksum" "$archive" | sha256sum --check --status
# Extract only the expected executable, never archive-controlled paths or links.
member=$(tar -tzf "$archive" | awk '$0 ~ /(^|\/)goose$/ {print; exit}')
[[ -n "$member" ]] || { echo 'Goose executable missing from archive.' >&2; exit 1; }
tar -xOzf "$archive" -- "$member" > "$install_dir/goose.new"
chmod 755 "$install_dir/goose.new"
[[ "$("$install_dir/goose.new" --version | xargs)" == "$version" ]]
mv -- "$install_dir/goose.new" "$install_dir/goose"
printf 'Installed verified Goose %s at %s/goose\n' "$version" "$install_dir"
