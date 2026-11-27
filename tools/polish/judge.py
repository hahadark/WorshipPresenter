# 외부 모델 판정 (OpenRouter, 그림 보는 모델). 키는 환경변수 OPENROUTER_API_KEY 에서만 읽고 어디에도 쓰지 않는다.
#   python -I tools/polish/judge.py <캡처 폴더> <결과 json> --model google/gemini-3.8-flash [--batch 8]
# 캡처는 1400px 폭 jpg 로 줄여 묶음마다 보낸다. 묶음마다 기준 7개 점수(1~10) + 지적 목록을 JSON 으로 받는다.
import base64, io, json, os, sys, time, urllib.request
from PIL import Image

def arg(n, d=None):
    return sys.argv[sys.argv.index(n) + 1] if n in sys.argv else d

src, dst = sys.argv[1], sys.argv[2]
model = arg('--model', 'google/gemini-3.8-flash')
batch = int(arg('--batch', '8'))
only = [x for x in (arg('--only', '') or '').split(',') if x]
key = os.environ.get('OPENROUTER_API_KEY')
if not key:
    sys.exit('OPENROUTER_API_KEY 없음')

RUBRIC = open(os.path.join(os.path.dirname(__file__), 'rubric.md'), encoding='utf-8').read()

files = sorted(f for f in os.listdir(src) if f.endswith('.png'))
if only:
    files = [f for f in files if any(f.startswith(o) for o in only)]
# 비슷한 화면끼리 묶음 (이름 앞부분 기준으로 정렬돼 있음)
groups = [files[i:i + batch] for i in range(0, len(files), batch)]

def jpg(path):
    im = Image.open(path).convert('RGB')
    w = 1400
    if im.width > w:
        im = im.resize((w, round(im.height * w / im.width)), Image.LANCZOS)
    b = io.BytesIO(); im.save(b, 'JPEG', quality=82)
    return base64.b64encode(b.getvalue()).decode()

out = {'model': model, 'groups': [], 'usage': {'prompt': 0, 'completion': 0, 'cost': 0.0}}
if os.path.exists(dst):
    out = json.load(open(dst, encoding='utf-8'))
done = {tuple(g['files']) for g in out['groups']}
for g in groups:
    if tuple(g) in done:
        continue
    content = [{'type': 'text', 'text': RUBRIC + '\n\n이번 묶음의 화면 (차례대로): ' + ', '.join(f[:-4] for f in g)}]
    for f in g:
        content.append({'type': 'text', 'text': f'[화면 {f[:-4]}]'})
        content.append({'type': 'image_url', 'image_url': {'url': 'data:image/jpeg;base64,' + jpg(os.path.join(src, f))}})
    body = {'model': model, 'messages': [{'role': 'user', 'content': content}], 'response_format': {'type': 'json_object'}, 'temperature': 0.2, 'usage': {'include': True}}
    req = urllib.request.Request('https://openrouter.ai/api/v1/chat/completions', data=json.dumps(body).encode(), headers={'Authorization': 'Bearer ' + key, 'Content-Type': 'application/json', 'X-Title': 'ui-review'})
    for attempt in range(3):
        try:
            r = json.load(urllib.request.urlopen(req, timeout=300))
            break
        except Exception as e:
            print('retry', e, file=sys.stderr); time.sleep(5)
    else:
        print('FAIL', g); continue
    txt = r['choices'][0]['message']['content']
    try:
        j = json.loads(txt[txt.find('{'):txt.rfind('}') + 1])
    except Exception:
        j = {'raw': txt}
    u = r.get('usage', {})
    out['usage']['prompt'] += u.get('prompt_tokens', 0)
    out['usage']['completion'] += u.get('completion_tokens', 0)
    out['usage']['cost'] += float(u.get('cost', 0) or 0)
    out['groups'].append({'files': g, 'result': j})
    json.dump(out, open(dst, 'w', encoding='utf-8'), ensure_ascii=False, indent=1)
    print(model, len(out['groups']), '/', len(groups), 'cost', round(out['usage']['cost'], 4), flush=True)
print('done', out['usage'])
