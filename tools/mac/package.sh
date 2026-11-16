#!/bin/bash
# 맥(Apple Silicon) 앱 묶음과 dmg 만들기. GitHub Actions(.github/workflows/mac.yml)에서 실행한다.
#   tools/mac/package.sh <버전> <bible.db 경로>
# 만드는 것: dist-mac/WorshipPresenter.app , dist-mac/WorshipPresenter-<버전>-mac-arm64.dmg
# 애플 개발자 인증서가 없으므로 임시 서명(ad-hoc)만 한다. 처음 열 때 Control+클릭 → 열기 가 필요하다.
set -euo pipefail
VER="$1"
SEED="$2"
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$ROOT"
OUT="$ROOT/dist-mac"
APP="$OUT/WorshipPresenter.app"
rm -rf "$OUT"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp target/release/wp-app "$APP/Contents/MacOS/WorshipPresenter"
cp target/release/wp-output "$APP/Contents/MacOS/wp-output"
cp -R web "$APP/Contents/Resources/web"
cp "$SEED" "$APP/Contents/Resources/bible.db"
cp LICENSE "$APP/Contents/Resources/LICENSE.txt"
cp THIRD_PARTY_NOTICES.md assets/fonts/OFL-Pretendard.txt "$APP/Contents/Resources/"
ICONSET="$OUT/WorshipPresenter.iconset"
mkdir -p "$ICONSET"
for s in 16 32 128 256 512; do
  sips -z $s $s assets/icon.png --out "$ICONSET/icon_${s}x${s}.png" >/dev/null
  d=$((s * 2))
  sips -z $d $d assets/icon.png --out "$ICONSET/icon_${s}x${s}@2x.png" >/dev/null
done
iconutil -c icns "$ICONSET" -o "$APP/Contents/Resources/WorshipPresenter.icns"
rm -rf "$ICONSET"
sed "s/__VERSION__/$VER/g" tools/mac/Info.plist > "$APP/Contents/Info.plist"
plutil -lint "$APP/Contents/Info.plist"
codesign --force -s - "$APP/Contents/MacOS/wp-output"
codesign --force -s - "$APP"
codesign --verify --verbose=2 "$APP"
STAGE="$OUT/dmg"
mkdir -p "$STAGE"
cp -R "$APP" "$STAGE/"
ln -s /Applications "$STAGE/Applications"
hdiutil create -volname "WorshipPresenter $VER" -srcfolder "$STAGE" -ov -format UDZO "$OUT/WorshipPresenter-$VER-mac-arm64.dmg"
rm -rf "$STAGE"
ls -la "$OUT"
