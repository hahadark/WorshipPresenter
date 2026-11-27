# 인터페이스 다듬기 2차: 같은 상태로 찍은 전·후 쌍을 나란히 (compare-<이름>.png) + 눈가림 쌍 비교 판정
#   python -I tools/polish/compare2.py tmp/p2 [--judge google/gemini-3.8-flash,openai/gpt-5.6-luna] [--max-cost 2]
# - 전·후 모두 tools/polish/shots.js 로 같은 인자 · 같은 시험 자료 · COMPUTERNAME=TEST-PC 로 찍은 것 (before = 2.1.0 공개판, after = 고친 판)
# - 왼쪽/오른쪽은 쌍마다 섞고(seed 11), 판정은 A/B → 전/후 로 되돌려 적는다. 키는 OPENROUTER_API_KEY 환경변수만
# - 보내기 전에 그림에 개인 정보가 없는지 사람이 compare-*.png 를 먼저 본다 (--judge 없이 한 번 돌려 확인)
import base64, io, json, os, random, sys, urllib.request
from PIL import Image, ImageDraw, ImageFont

d = sys.argv[1]
SHOTS = ['main-1600', 'main-1000', 'main-s150', 'main-notify', 'edit-actions', 'menu-도구', 'menu-보기', 'win-tpl',
         'win-c-themes', 'win-b-cues', 'win-ai-listen', 'tool-devices', 'win-b-macros', 'win-c-settings']
# 같은 창이 아닌 참고 쌍: 옛 '밝게' 와 새 '전문가 밝게' (같은 상태)
EXTRA = [('main-light', 'main-prolight')]
font = None
for f in ['C:/Windows/Fonts/malgunbd.ttf', 'C:/Windows/Fonts/malgun.ttf']:
    if os.path.exists(f):
        font = ImageFont.truetype(f, 28)
        break


def side(a, b, la, lb, w=1200):
    ims = []
    for p in (a, b):
        im = Image.open(p).convert('RGB')
        im.thumbnail((w, 10000))
        ims.append(im)
    h = max(i.height for i in ims) + 50
    out = Image.new('RGB', (w * 2 + 20, h), (20, 20, 22))
    dr = ImageDraw.Draw(out)
    for k, (im, lab) in enumerate(zip(ims, (la, lb))):
        x = k * (w + 20)
        out.paste(im, (x, 50))
        dr.text((x + 10, 8), lab, fill=(240, 240, 240), font=font)
    return out


pairs = [(s, f'{d}/before/{s}.png', f'{d}/after/{s}.png') for s in SHOTS]
pairs += [(f'{a}→{b}', f'{d}/before/{a}.png', f'{d}/after/{b}.png') for a, b in EXTRA]
if '--only' in sys.argv:  # 고친 뒤 몇 쌍만 다시 판정 (결과는 pairwise-<이름>.json)
    only = sys.argv[sys.argv.index('--only') + 1].split(',')
    pairs = [p for p in pairs if p[0] in only]
for name, a, b in pairs:
    side(a, b, f'전 (2.1.0) — {name}', f'후 (다듬기 2차) — {name}').save(f'{d}/compare-{name}.png')
print('compare images', len(pairs))

if '--judge' not in sys.argv:
    sys.exit()
models = sys.argv[sys.argv.index('--judge') + 1].split(',')
max_cost = float(sys.argv[sys.argv.index('--max-cost') + 1]) if '--max-cost' in sys.argv else 2.0
key = os.environ['OPENROUTER_API_KEY']
random.seed(11)
ASK = open(os.path.join(os.path.dirname(__file__), 'rubric.md'), encoding='utf-8').read().split('Score each')[0] + '''
You are shown the SAME screen of the app in two versions, A (left) and B (right), captured in the same state. Judge only the app chrome (controls, labels, colors, spacing), not slide content or the clock value.
For each criterion answer which version is better: "A", "B" or "same". Reply JSON only:
{"consistency": "A|B|same", "color_meaning": ..., "hierarchy": ..., "ai_look": ..., "pro_tool": ..., "wording": ..., "usability": ..., "overall": "A|B|same", "why": "<one Korean sentence>"}'''
res = {'pairs': [], 'cost': 0.0}
for name, before, after in pairs:
    flip = random.random() < 0.5  # True 면 왼쪽 = 후
    a, b = (after, before) if flip else (before, after)
    im = side(a, b, 'A', 'B', 1000)
    buf = io.BytesIO(); im.save(buf, 'JPEG', quality=82)
    url = 'data:image/jpeg;base64,' + base64.b64encode(buf.getvalue()).decode()
    for m in models:
        if res['cost'] >= max_cost:
            print('비용 한도에 닿아 멈춤', res['cost'])
            break
        body = {'model': m, 'messages': [{'role': 'user', 'content': [{'type': 'text', 'text': ASK}, {'type': 'image_url', 'image_url': {'url': url}}]}], 'response_format': {'type': 'json_object'}, 'temperature': 0.2, 'usage': {'include': True}}
        req = urllib.request.Request('https://openrouter.ai/api/v1/chat/completions', data=json.dumps(body).encode(), headers={'Authorization': 'Bearer ' + key, 'Content-Type': 'application/json'})
        try:
            r = json.load(urllib.request.urlopen(req, timeout=300))
        except Exception as e:
            print(name, m, 'ERROR', e, flush=True)
            continue
        t = r['choices'][0]['message']['content'] or ''
        try:
            j = json.loads(t[t.find('{'):t.rfind('}') + 1])
        except Exception:
            j = {'raw': t}
        m2 = {k: (('after' if (v == 'A') == flip else 'before') if v in ('A', 'B') else v) for k, v in j.items()}
        res['cost'] += float(r.get('usage', {}).get('cost', 0) or 0)
        res['pairs'].append({'shot': name, 'model': m, 'left_is_after': flip, 'verdict': m2})
        print(name, m, m2.get('overall'), m2.get('why', '')[:90], flush=True)
out = f'{d}/pairwise.json' if '--only' not in sys.argv else f"{d}/pairwise-{sys.argv[sys.argv.index('--only') + 1].replace(',', '_')}.json"
json.dump(res, open(out, 'w', encoding='utf-8'), ensure_ascii=False, indent=1)
print('cost', round(res['cost'], 4))
