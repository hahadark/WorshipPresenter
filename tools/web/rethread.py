# [웹판] 문장 자리의 std::thread::spawn(move || …) 를 wp_core::bg::spawn 으로 바꾼다 (되풀이해 돌려도 같다).
# 까닭: 브라우저(wasm32)에서 std::thread::spawn 은 panic → 조작 화면 전체가 멈춘다. bg::spawn 은 설치판에서는 std 그대로,
#       웹에서는 돌리지 않고 '설치판에서 해 주세요' 를 알린다. 결과(JoinHandle)를 쓰는 자리 · 설치판 전용 파일 · 시험은 건드리지 않는다.
# 다른 가지를 합친 뒤:  python tools/web/rethread.py
import re, pathlib
root = pathlib.Path(__file__).resolve().parents[2]
SKIP = {"crates/wp-app/src/main.rs", "crates/wp-app/src/outwin.rs", "crates/wp-core/src/flow.rs", "crates/wp-core/src/settings.rs",
        "crates/wp-core/src/lockcheck.rs", "crates/wp-core/src/companion.rs", "crates/wp-core/src/bg.rs",
        "crates/wp-core/src/lyricsweb.rs"}  # JoinHandle 을 모아 join 하는 곳
n = 0
for d, path in (("crates/wp-app/src", "wp_core::bg::spawn("), ("crates/wp-core/src", "crate::bg::spawn(")):
    for f in (root / d).rglob("*.rs"):
        rel = f.relative_to(root).as_posix()
        if rel in SKIP or "/web/" in rel:
            continue
        s = f.read_bytes().decode("utf-8")
        # 시험 모듈(#[cfg(test)] 아래)은 빼려고 그 앞까지만 바꾼다
        cut = s.find("#[cfg(test)]\nmod ")
        head, tail = (s, "") if cut < 0 else (s[:cut], s[cut:])
        t = re.sub(r"(?m)^(\s*)std::thread::spawn\(move \|\|", lambda m: m.group(1) + path + "move ||", head)
        if t != head:
            n += t.count(path) - head.count(path)
            f.write_bytes((t + tail).encode("utf-8"))
print(f"바꾼 곳: {n}")
