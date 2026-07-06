#!/bin/bash
# Build md-reader and wrap it in a macOS .app so double-clicking a .md file in
# Finder renders it to a self-contained HTML file and opens it in the browser.
# There is no server and no background process: the helper exits immediately
# after opening the page.
#
# Usage:  ./install-macos.sh            # build + create the app in ./dist
#         ./install-macos.sh --default  # also make it the default .md handler
set -euo pipefail

cd "$(dirname "$0")"

APP_NAME="md-reader"
APP_DIR="dist/${APP_NAME}.app"

echo "==> Building release binary"
cargo build --release

echo "==> Creating ${APP_DIR}"
rm -rf "$APP_DIR"
mkdir -p dist

# AppleScript wrapper: Finder passes the opened file(s) to the `on open`
# handler; we shell out to the bundled binary, which renders the file and opens
# the browser. It exits on its own, so no backgrounding or cleanup is needed.
SCRIPT=$(mktemp)
trap 'rm -f "$SCRIPT"' EXIT
cat > "$SCRIPT" <<'OSA'
on open theFiles
	set binPath to POSIX path of (path to resource "md-reader")
	repeat with f in theFiles
		set p to POSIX path of f
		do shell script quoted form of binPath & " " & quoted form of p
	end repeat
end open

on run
	display dialog "md-reader is a helper. Open a .md file in Finder (right-click ▸ Open With) to use it." buttons {"OK"} default button 1 with icon note
end run
OSA

osacompile -o "$APP_DIR" "$SCRIPT"

# Bundle the binary as an app resource so the .app is self-contained.
cp target/release/md-reader "$APP_DIR/Contents/Resources/md-reader"
chmod +x "$APP_DIR/Contents/Resources/md-reader"

# Declare that this app handles Markdown documents, and give it a stable
# bundle id so Launch Services can target it.
PLIST="$APP_DIR/Contents/Info.plist"
plutil -replace CFBundleIdentifier -string com.md-reader.viewer "$PLIST"
plutil -replace CFBundleDocumentTypes -json '[{
  "CFBundleTypeName": "Markdown",
  "CFBundleTypeRole": "Viewer",
  "LSHandlerRank": "Alternate",
  "LSItemContentTypes": ["net.daringfireball.markdown", "public.markdown"]
}]' "$PLIST"

# The plist edits invalidate osacompile's ad-hoc signature; re-sign so the
# bundle still verifies (matters if the app is ever copied to another Mac).
codesign --force -s - "$APP_DIR"

# Install into ~/Applications and register with Launch Services.
DEST="$HOME/Applications/${APP_NAME}.app"
echo "==> Installing to ${DEST}"
mkdir -p "$HOME/Applications"
rm -rf "$DEST"
cp -R "$APP_DIR" "$DEST"
/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister -f "$DEST"

echo "==> Done."
echo
echo "Right-click any .md in Finder ▸ Open With ▸ ${APP_NAME}."
echo

if [[ "${1:-}" == "--default" ]]; then
  if command -v duti >/dev/null 2>&1; then
    echo "==> Setting ${APP_NAME} as the default handler for Markdown"
    duti -s com.md-reader.viewer net.daringfireball.markdown all || true
    duti -s com.md-reader.viewer public.markdown all || true
    duti -s com.md-reader.viewer .md all || true
    echo "Done. Double-clicking a .md now opens ${APP_NAME}."
  else
    echo "To make it the DEFAULT for all .md files:"
    echo "  • Right-click a .md ▸ Get Info ▸ 'Open with' ▸ choose ${APP_NAME} ▸ 'Change All…'"
    echo "  • or install duti (brew install duti) and re-run: ./install-macos.sh --default"
  fi
fi
