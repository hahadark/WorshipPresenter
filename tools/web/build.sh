#!/usr/bin/env bash
# [웹판] 조작 화면을 브라우저용으로 빌드: bash tools/web/build.sh [--dev]
# 결과: dist-web/ (index.html · wp-app.js · wp-app_bg.wasm(+.br) · outputs/ 등 web/ 출력 페이지)
# 띄워 보기: node tools/web/serve.js dist-web 37610 → http://127.0.0.1:37610/
set -euo pipefail
cd "$(dirname "$0")/../.."
source tools/web/env.sh
PROFILE=release; [ "${1:-}" = "--dev" ] && PROFILE=debug
FLAGS=(--offline); [ "$PROFILE" = release ] && FLAGS+=(--release)
cargo build -p wp-app --target wasm32-unknown-unknown "${FLAGS[@]}"
OUT=dist-web
rm -rf "$OUT"; mkdir -p "$OUT"
wasm-bindgen --target web --no-typescript --out-dir "$OUT" "target/wasm32-unknown-unknown/$PROFILE/wp-app.wasm"
if [ "$PROFILE" = release ]; then
  wasm-opt -O2 --enable-bulk-memory --enable-nontrapping-float-to-int --enable-sign-ext --enable-mutable-globals --enable-reference-types --enable-multivalue "$OUT/wp-app_bg.wasm" -o "$OUT/wp-app_bg.opt.wasm"
  mv "$OUT/wp-app_bg.opt.wasm" "$OUT/wp-app_bg.wasm"
fi
cp web/app/index.html web/app/media-glue.js "$OUT/"
# 출력 페이지(예배당 · 방송 · 무대)는 설치판과 같은 web/ 를 그대로 쓰고, <head> 바로 뒤에 같은 브라우저 어댑터만 끼운다
cp -r web/shared "$OUT/shared"
mkdir -p "$OUT/outputs" "$OUT/app"
cp web/output/output.js web/output/stage.js "$OUT/outputs/"
cp web/app/local-live.js "$OUT/app/"
inject() { node -e "const fs=require('fs');fs.writeFileSync(process.argv[2],fs.readFileSync(process.argv[1],'utf8').replace('<head>','<head>\n<script src=\"/app/local-live.js\"></script>'))" "$1" "$2"; }
inject web/output/index.html "$OUT/outputs/main.html"
inject web/output/index.html "$OUT/outputs/overlay.html"
inject web/output/stage.html "$OUT/outputs/stage.html"
[ -d web/app/sample ] && cp -r web/app/sample "$OUT/sample"
# 조작 화면 글꼴(Pretendard, OFL)은 wasm 에 넣지 않고 따로 (web/mod.rs fonts) — 첫 열기 크기 · 따로 캐시
mkdir -p "$OUT/fonts" && cp assets/fonts/Pretendard-*.ttf assets/fonts/OFL-Pretendard.txt "$OUT/fonts/"
# Cloudflare 처럼 압축해 보내는 크기를 재려고 미리 brotli 로 눌러 둔다
node -e "const z=require('zlib'),fs=require('fs');for(const f of process.argv.slice(1)){const b=fs.readFileSync(f);fs.writeFileSync(f+'.br',z.brotliCompressSync(b,{params:{[z.constants.BROTLI_PARAM_QUALITY]:11,[z.constants.BROTLI_PARAM_SIZE_HINT]:b.length}}));console.log(f,b.length,'→ br',fs.statSync(f+'.br').size)}" "$OUT/wp-app_bg.wasm" "$OUT/wp-app.js" "$OUT"/fonts/*.ttf
