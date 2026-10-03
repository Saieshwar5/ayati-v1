#!/bin/bash

set -euo pipefail

repo_root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$repo_root"

scope="${1:-all}"
if [[ "$scope" != "all" && "$scope" != "ios" ]]; then
  echo "usage: $0 [ios]" >&2
  exit 2
fi

./scripts/check-swift-tools.sh swiftformat

node scripts/ios-write-swift-filelist.mjs
(
  cd apps/ios
  swiftformat --lint \
    --config ../../config/swiftformat \
    --unexclude "$PWD/Sources,$PWD/ShareExtension,$PWD/ActivityWidget,$PWD/WatchApp,$PWD/../shared/OpenClawKit/Sources/OpenClawChatUI,$PWD/../shared/OpenClawKit/Sources/OpenClawKit,$PWD/../shared/OpenClawKit/Sources/OpenClawNativeState,$PWD/../shared/OpenClawKit/Sources/OpenClawProtocol,$PWD/../swabble/Sources/SwabbleKit" \
    --filelist SwiftSources.input.xcfilelist
)
