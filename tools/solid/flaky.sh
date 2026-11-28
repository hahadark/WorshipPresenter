#!/bin/bash
# 흔들리는 시험 찾기: 시험 바이너리를 순서 섞어 · 스레드 수를 바꿔 가며 N번 돌리고 실패한 시험 이름을 모은다
# 사용: flaky.sh <반복 수> <결과 폴더> [app|core|both]
cd "$(dirname "$0")/../.."
N=${1:-10}; OUT=${2:-tmp/solid/flaky}; WHICH=${3:-both}
mkdir -p "$OUT"
# 시험 실행 파일을 복사해서 돌린다 (도는 동안에도 cargo 빌드가 실행 파일을 덮어쓸 수 있게)
mkdir -p "$OUT/bin"
cp "$(ls -t target/debug/deps/wp_app-*.exe | head -1)" "$OUT/bin/wp_app-test.exe"
cp "$(ls -t target/debug/deps/wp_core-*.exe | head -1)" "$OUT/bin/wp_core-test.exe"
APP="$OUT/bin/wp_app-test.exe"
CORE="$OUT/bin/wp_core-test.exe"
export RUSTC_BOOTSTRAP=1
# 실제 1.x 자료를 보지 않게 (점검 격리)
export WP_LEGACY_DATA="$(cygpath -w "$PWD/tmp/solid/empty-legacy")"; mkdir -p tmp/solid/empty-legacy
: > "$OUT/summary.txt"
for i in $(seq 1 $N); do
  for T in 12 32 3; do
    seed=$((RANDOM * 32768 + RANDOM))
    if [ "$WHICH" != core ]; then
      (cd crates/wp-app && "../../$APP" -Z unstable-options --shuffle-seed $seed --test-threads $T > "../../$OUT/app-$i-$T.log" 2>&1); rc=$?
      echo "app iter=$i threads=$T seed=$seed rc=$rc $(grep -E '^test result' "$OUT/app-$i-$T.log" | tail -1)" >> "$OUT/summary.txt"
      [ $rc -eq 0 ] && rm -f "$OUT/app-$i-$T.log"
    fi
    if [ "$WHICH" != app ]; then
      (cd crates/wp-core && "../../$CORE" -Z unstable-options --shuffle-seed $seed --test-threads $T > "../../$OUT/core-$i-$T.log" 2>&1); rc=$?
      echo "core iter=$i threads=$T seed=$seed rc=$rc $(grep -E '^test result' "$OUT/core-$i-$T.log" | tail -1)" >> "$OUT/summary.txt"
      [ $rc -eq 0 ] && rm -f "$OUT/core-$i-$T.log"
    fi
  done
done
echo DONE >> "$OUT/summary.txt"
