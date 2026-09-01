#!/usr/bin/env bash
# Register the ModSearch native-messaging host for Chrome/Chromium.
# Usage: install-host.sh <path-to-engine-binary> <extension-id>
set -euo pipefail
ENGINE="${1:?usage: install-host.sh <path-to-engine-binary> <extension-id>}"
EXT_ID="${2:?usage: install-host.sh <path-to-engine-binary> <extension-id>}"
NAME="com.modsearch.engine"
case "$(uname -s)" in
  Darwin) DIR="$HOME/Library/Application Support/Google/Chrome/NativeMessagingHosts" ;;
  Linux)  DIR="$HOME/.config/google-chrome/NativeMessagingHosts" ;;
  *) echo "Unsupported OS: $(uname -s)"; exit 1 ;;
esac
mkdir -p "$DIR"
cat > "$DIR/$NAME.json" <<JSON
{
  "name": "$NAME",
  "description": "ModSearch on-device engine",
  "path": "$ENGINE",
  "type": "stdio",
  "allowed_origins": ["chrome-extension://$EXT_ID/"]
}
JSON
echo "Installed $DIR/$NAME.json"
echo "  engine: $ENGINE"
echo "  ext id: $EXT_ID"
echo "Restart Chrome for it to pick up the host."
