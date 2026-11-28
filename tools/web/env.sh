# 웹판(wasm32) 빌드 환경 — source tools/web/env.sh
# clang: SQLite(C)를 wasm32 로 컴파일할 때 필요 (wasi-sdk 의 clang 을 설치 없이 풀어 둔 것)
export PATH="$USERPROFILE/.cargo/bin:$PATH"
WP_WEB_TOOLS="${WP_WEB_TOOLS:-$LOCALAPPDATA/wp-web-tools}"
WASI="$(ls -d "$WP_WEB_TOOLS"/wasi-sdk-*/ 2>/dev/null | head -1)"
export CC_wasm32_unknown_unknown="$(cygpath -w "${WASI}bin/clang.exe")"
export AR_wasm32_unknown_unknown="$(cygpath -w "${WASI}bin/llvm-ar.exe")"
export PATH="$(ls -d "$WP_WEB_TOOLS"/wasm-bindgen-*/ | head -1):$(ls -d "$WP_WEB_TOOLS"/binaryen-*/bin | head -1):$PATH"
