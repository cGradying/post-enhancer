#!/usr/bin/env bash
# Build "Post Prep.app" into ~/Applications so it opens from Finder, Launchpad or Spotlight.
# Opening the app starts the local server (if it isn't running) and opens it in your browser.
set -euo pipefail
cd "$(dirname "$0")"

APP="$HOME/Applications/Post Prep.app"
cargo build --release

rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp target/release/photo-post-prep "$APP/Contents/Resources/"

cat > "$APP/Contents/MacOS/post-prep" <<'EOF'
#!/bin/bash
# Local only: binds 127.0.0.1, no password, batches kept in Application Support.
PORT=4870
URL="http://127.0.0.1:$PORT"
DATA="$HOME/Library/Application Support/Post Prep"
mkdir -p "$DATA"
if ! curl -s -o /dev/null "$URL/login"; then
  HOST=127.0.0.1 PORT=$PORT DATA_DIR="$DATA/batches" \
    nohup "$(dirname "$0")/../Resources/photo-post-prep" >>"$DATA/server.log" 2>&1 &
  for _ in $(seq 50); do curl -s -o /dev/null "$URL/login" && break; sleep 0.1; done
fi
open "$URL"
EOF
chmod +x "$APP/Contents/MacOS/post-prep"

cat > "$APP/Contents/Info.plist" <<'EOF'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>CFBundleName</key><string>Post Prep</string>
  <key>CFBundleDisplayName</key><string>Post Prep</string>
  <key>CFBundleIdentifier</key><string>local.photo-assistant.post-prep</string>
  <key>CFBundleExecutable</key><string>post-prep</string>
  <key>CFBundleIconFile</key><string>icon</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>1.0</string>
  <key>LSUIElement</key><true/>
</dict></plist>
EOF

# Icon: a purple pixel fade with three white panels, drawn on a 16x16 grid.
ICONSET="$(mktemp -d)/icon.iconset"; mkdir -p "$ICONSET"
python3 - "$ICONSET/icon_512x512@2x.png" <<'EOF'
import sys, zlib, struct, random
N, S = 16, 64  # 16x16 cells, 64px each = 1024px
random.seed(7)
ramp = [(44,30,78),(90,60,170),(124,58,237),(166,107,255),(217,190,255)]
cells = []
for y in range(N):
    row = []
    for x in range(N):
        t = min(1, max(0, (x + (N - y)) / (2 * N) * 1.3 - .1 + (random.random() - .5) * .3))
        k = t * (len(ramp) - 1); i = int(k); f = k - i
        a, b = ramp[i], ramp[min(i + 1, len(ramp) - 1)]
        row.append(tuple(int(a[c] + (b[c] - a[c]) * f) for c in range(3)))
    cells.append(row)
for x0 in (3, 7, 11):  # three carousel panels
    for y in range(5, 11):
        for x in range(x0, x0 + 3):
            cells[y][x] = (255, 255, 255)
rows = b''.join(b'\0' + b''.join(bytes(cells[y // S][x // S]) for x in range(N * S)) for y in range(N * S))
chunk = lambda t, d: struct.pack('>I', len(d)) + t + d + struct.pack('>I', zlib.crc32(t + d))
png = b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('>IIBBBBB', N * S, N * S, 8, 2, 0, 0, 0)) + chunk(b'IDAT', zlib.compress(rows, 9)) + chunk(b'IEND', b'')
open(sys.argv[1], 'wb').write(png)
EOF
for s in 16 32 128 256 512; do
  sips -z $s $s "$ICONSET/icon_512x512@2x.png" --out "$ICONSET/icon_${s}x${s}.png" >/dev/null
  sips -z $((s*2)) $((s*2)) "$ICONSET/icon_512x512@2x.png" --out "$ICONSET/icon_${s}x${s}@2x.png" >/dev/null
done
iconutil -c icns "$ICONSET" -o "$APP/Contents/Resources/icon.icns"
codesign --force --deep -s - "$APP" 2>/dev/null || true
touch "$APP"

echo "Installed: $APP"
echo "To stop the background server: pkill -f 'Post Prep.app/Contents/Resources/photo-post-prep'"
