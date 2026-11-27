# 전후 점수표: 모델 · 기준마다 3번 판정의 평균 (± 표준편차) · 비용 합
import json, glob, statistics as st, sys
K = ['consistency', 'color_meaning', 'hierarchy', 'ai_look', 'pro_tool', 'wording', 'usability']
NAME = {'consistency': '일관성', 'color_meaning': '색 뜻', 'hierarchy': '위계', 'ai_look': 'AI 티 없음', 'pro_tool': '전문가 도구다움', 'wording': '문구', 'usability': '사용성'}
d = sys.argv[1] if len(sys.argv) > 1 else 'tmp/polish'
cost = 0.0
rows = {}
for m in ['gemini', 'gpt']:
    for w in ['before', 'after']:
        runs = []
        for f in sorted(glob.glob(f'{d}/judge-{w}-{m}-r*.json')):
            j = json.load(open(f, encoding='utf-8'))
            cost += j['usage']['cost']
            gs = [g['result']['scores'] for g in j['groups'] if 'scores' in g['result']]
            runs.append({k: st.mean(float(g.get(k, 0)) for g in gs) for k in K})
        rows[(m, w)] = runs
print('| 기준 | Gemini 전 | Gemini 후 | GPT 전 | GPT 후 |')
print('|---|---|---|---|---|')
for k in K + ['avg']:
    cells = []
    for m in ['gemini', 'gpt']:
        for w in ['before', 'after']:
            vals = [st.mean(r.values()) if k == 'avg' else r[k] for r in rows[(m, w)]]
            cells.append(f'{st.mean(vals):.2f} ±{st.pstdev(vals):.2f}')
    print(f"| {'평균' if k == 'avg' else NAME[k]} | " + ' | '.join(cells) + ' |')
print('runs', {f'{m}-{w}': len(v) for (m, w), v in rows.items()}, 'cost', round(cost, 3))
