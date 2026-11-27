# 전후 나란히 그림 (tmp/polish/compare-<이름>.png) + 눈가림 쌍 비교 판정 (어느 쪽이 나은가, 왼쪽/오른쪽은 섞음)
#   python -I tools/polish/compare.py tmp/polish [--judge google/gemini-3.8-flash,openai/gpt-5.6-luna]
import base64, io, json, os, random, sys, urllib.request
from PIL import Image, ImageDraw, ImageFont

d = sys.argv[1]
SHOTS = ['win-c-settings', 'win-d-output', 'tool-devices', 'win-b-diagnose', 'win-b-macros', 'en-scene', 'win-c-about', 'win-b-lib']
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


for s in SHOTS:
    side(f'{d}/before/{s}.png', f'{d}/after/{s}.png', f'전 (before) — {s}', f'후 (after) — {s}').save(f'{d}/compare-{s}.png')
print('compare images', len(SHOTS))

if '--judge' not in sys.argv:
    sys.exit()
models = sys.argv[sys.argv.index('--judge') + 1].split(',')
key = os.environ['OPENROUTER_API_KEY']
random.seed(7)
ASK = open(os.path.join(os.path.dirname(__file__), 'rubric.md'), encoding='utf-8').read().split('Score each')[0] + '''
You are shown the SAME window of the app in two versions, A (left) and B (right). Judge only the app chrome around and inside that window (not slide content).
For each criterion answer which version is better: "A", "B" or "same". Reply JSON only:
{"consistency": "A|B|same", "color_meaning": ..., "hierarchy": ..., "ai_look": ..., "pro_tool": ..., "wording": ..., "usability": ..., "overall": "A|B|same", "why": "<one Korean sentence>"}'''
res = {'pairs': [], 'cost': 0.0}
for s in SHOTS:
    flip = random.random() < 0.5  # True 면 왼쪽 = 후
    a, b = (f'{d}/after/{s}.png', f'{d}/before/{s}.png') if flip else (f'{d}/before/{s}.png', f'{d}/after/{s}.png')
    im = side(a, b, 'A', 'B', 1000)
    buf = io.BytesIO(); im.save(buf, 'JPEG', quality=82)
    url = 'data:image/jpeg;base64,' + base64.b64encode(buf.getvalue()).decode()
    for m in models:
        body = {'model': m, 'messages': [{'role': 'user', 'content': [{'type': 'text', 'text': ASK}, {'type': 'image_url', 'image_url': {'url': url}}]}], 'response_format': {'type': 'json_object'}, 'temperature': 0.2, 'usage': {'include': True}}
        req = urllib.request.Request('https://openrouter.ai/api/v1/chat/completions', data=json.dumps(body).encode(), headers={'Authorization': 'Bearer ' + key, 'Content-Type': 'application/json'})
        r = json.load(urllib.request.urlopen(req, timeout=300))
        t = r['choices'][0]['message']['content']
        try:
            j = json.loads(t[t.find('{'):t.rfind('}') + 1])
        except Exception:
            j = {'raw': t}
        # A/B → 전/후 로 되돌림
        m2 = {}
        for k, v in j.items():
            if v in ('A', 'B'):
                m2[k] = 'after' if (v == 'A') == flip else 'before'
            else:
                m2[k] = v
        res['cost'] += float(r.get('usage', {}).get('cost', 0) or 0)
        res['pairs'].append({'shot': s, 'model': m, 'verdict': m2})
        print(s, m, m2.get('overall'), m2.get('why', '')[:80], flush=True)
json.dump(res, open(f'{d}/pairwise.json', 'w', encoding='utf-8'), ensure_ascii=False, indent=1)
print('cost', round(res['cost'], 4))
