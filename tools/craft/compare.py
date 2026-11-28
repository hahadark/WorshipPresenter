# 전후 쌍 그림: 같은 이름의 캡처를 왼쪽(전) · 오른쪽(후)으로 붙인다
#   python tools/craft/compare.py <전 폴더> <후 폴더> <출력 폴더> 이름 이름 ...   → <출력>/compare-<이름>.png
import sys, os
from PIL import Image, ImageDraw
a, b, out, *names = sys.argv[1:]
os.makedirs(out, exist_ok=True)
for n in names:
    pa, pb = os.path.join(a, n + '.png'), os.path.join(b, n + '.png')
    if not (os.path.exists(pa) and os.path.exists(pb)):
        print('없음', n); continue
    ia, ib = Image.open(pa).convert('RGB'), Image.open(pb).convert('RGB')
    w = 1200
    ia = ia.resize((w, int(ia.size[1] * w / ia.size[0]))); ib = ib.resize((w, int(ib.size[1] * w / ib.size[0])))
    sh = Image.new('RGB', (w * 2 + 10, max(ia.size[1], ib.size[1]) + 26), 'white')
    sh.paste(ia, (0, 26)); sh.paste(ib, (w + 10, 26))
    d = ImageDraw.Draw(sh)
    d.text((6, 6), n + '  before', fill=(0, 0, 0)); d.text((w + 16, 6), n + '  after', fill=(0, 0, 0))
    sh.save(os.path.join(out, 'compare-' + n + '.png'))
    print('ok', n)
