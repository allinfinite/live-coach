#!/bin/zsh
set -euo pipefail
base="$(cd -- "$(dirname -- "$0")/.." && pwd)"
source_app="$base/dist/Live Coach.app"
destination="$HOME/Applications/Live Coach.app"
[[ -d "$source_app" ]] || { print -u2 "Missing packaged app: $source_app"; exit 1; }
if [[ -e "$destination" ]]; then
  if cmp -s "$source_app/Contents/MacOS/live-coach" "$destination/Contents/MacOS/live-coach" && cmp -s "$source_app/Contents/_CodeSignature/CodeResources" "$destination/Contents/_CodeSignature/CodeResources"; then
    codesign --verify --deep --strict "$destination"
    open "$destination"
    print "This build is already installed at $destination"
    exit 0
  fi
  print -u2 "A different Live Coach already exists at $destination. Move it aside before installing this build."
  exit 1
fi
mkdir -p "$HOME/Applications"
ditto --norsrc --noextattr "$source_app" "$destination"
codesign --verify --deep --strict "$destination"
open "$destination"
print "Installed $destination"
