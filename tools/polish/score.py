# 판정 결과 요약: 기준별 평균 점수 (모델마다) · 비용
import json, sys
K = ['consistency', 'color_meaning', 'hierarchy', 'ai_look', 'pro_tool', 'wording', 'usability']
for f in sys.argv[1:]:
    d = json.load(open(f, encoding='utf-8'))
    gs = [g['result'].get('scores', {}) for g in d['groups'] if 'scores' in g['result']]
    avg = {k: round(sum(float(g.get(k, 0)) for g in gs) / max(1, len(gs)), 2) for k in K}
    tot = round(sum(avg.values()) / len(K), 2)
    n = sum(len(g['result'].get('issues', [])) for g in d['groups'])
    print(json.dumps({'file': f, 'model': d['model'], 'groups': len(gs), 'avg': avg, 'overall': tot, 'issues': n, 'cost': round(d['usage']['cost'], 4)}, ensure_ascii=False))
