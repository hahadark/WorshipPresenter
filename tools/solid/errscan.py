"""사용자에게 보이는 오류 · 경고 글 뽑기 (알림 · bail! · anyhow! · Err(format!) · context).
영어 사전에 없는 한국어 글, 한국어가 없는 글(영어만)을 따로 보인다. 결과 전체는 tmp/solid/errors.tsv
사용: python tools/solid/errscan.py
"""
import glob
import json
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
BS = chr(92)
STR = '"((?:[^"' + BS + BS + ']|' + BS + BS + '.)*)"'
pat = re.compile(r'(notify(?:_act)?\(|bail!\(|anyhow!\(|Err\(format!\(|err\(format!\(|\.context\()\s*(?:[\w:]*tr(?:_args|_s)?\()?\s*' + STR)
out = []
for f in glob.glob(str(ROOT / 'crates/wp-*/src/**/*.rs'), recursive=True):
    if '_tests' in f:
        continue
    txt = open(f, encoding='utf-8', errors='replace').read()
    cut = txt.find('#[cfg(test)]')
    for m in pat.finditer(txt):
        if 0 <= cut < m.start():
            continue
        s = m.group(2)
        rel = str(Path(f).relative_to(ROOT)).replace(BS, '/')
        out.append((rel, txt[:m.start()].count('\n') + 1, '' if re.search('[가-힣]', s) else 'EN', s))
d = json.load(open(ROOT / 'crates/wp-core/data/en.json', encoding='utf-8'))
(ROOT / 'tmp/solid').mkdir(parents=True, exist_ok=True)
(ROOT / 'tmp/solid/errors.tsv').write_text('\n'.join(f'{a}:{b}\t{c}\t{s}' for a, b, c, s in out), encoding='utf-8')
miss = [o for o in out if o[2] == '' and o[3] not in d]
en = [o for o in out if o[2] == 'EN' and len(o[3]) > 3]
print('all', len(out))
print('no-en', len(miss))
for o in miss:
    print(f'{o[0]}:{o[1]}  {o[3][:120]}')
print('ENGLISH-only', len(en))
for o in en:
    print(f'{o[0]}:{o[1]}  {o[3][:110]}')
