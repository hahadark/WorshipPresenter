# [웹판] std::time::Instant 를 web_time::Instant 로 바꾼다 (되풀이해 돌려도 같다).
# SystemTime 은 파일 시각(metadata().modified())과 섞여 쓰여 통째로 못 바꾼다 → SystemTime::now() 자리만 손으로 (util::now_ms 등).
# 까닭: 브라우저(wasm32-unknown-unknown)에서 std 의 Instant::now() · SystemTime::now() 는 panic 한다.
#       web_time 은 설치판에서는 std::time 을 그대로 다시 내보내므로(같은 타입) 설치판 동작은 바뀌지 않는다.
# 다른 가지를 합친 뒤 새로 생긴 std::time::Instant 가 있으면 이 스크립트를 다시 돌린다:  python tools/web/retime.py
import re, pathlib, sys
root = pathlib.Path(__file__).resolve().parents[2]
n = 0
for d in ("crates/wp-core/src", "crates/wp-app/src"):
    for f in (root / d).rglob("*.rs"):
        if "web" in f.relative_to(root / d).parts[:1]:
            continue
        b = f.read_bytes()
        s = b.decode("utf-8")
        t = re.sub(r"\bstd::time::(Instant)\b", r"web_time::\1", s)
        t = re.sub(r"\buse std::time::(\{[^}]*\b(?:Instant)\b[^}]*\});", r"use web_time::\1;", t)
        if t != s:
            n += len([1 for a, c in zip(s.splitlines(), t.splitlines()) if a != c])
            f.write_bytes(t.encode("utf-8"))
print(f"바꾼 줄: {n}")
