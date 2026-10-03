#!/bin/bash

set -euo pipefail

repo_root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$repo_root"

scope="${1:-all}"
if [[ "$scope" != "all" && "$scope" != "ios" ]]; then
  echo "usage: $0 [ios]" >&2
  exit 2
fi

./scripts/check-swift-tools.sh swiftlint

(
  cd apps/ios
  node "$repo_root/scripts/run-swiftlint.mts" --strict --config .swiftlint.yml
)
