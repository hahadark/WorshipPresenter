# 창만 잘라 내기: 창을 연 캡처에서 가장 큰 '창 제목 줄' 을 찾는 대신, 같은 크기 바탕 캡처(main.png)와 달라진 곳의 테두리 상자로 자른다.
#   python tools/craft/crop.py tmp/craft/before [이름 ...]   → tmp/craft/before/crop/<이름>.png
import sys, os
from PIL import Image, ImageChops
d = sys.argv[1]
names = sys.argv[2:] or [f[:-4] for f in os.listdir(d) if f.endswith('.png')]
out = os.path.join(d, 'crop'); os.makedirs(out, exist_ok=True)
base = {}
def base_for(size):
    if size not in base:
        p = os.path.join(d, 'main.png')
        base[size] = Image.open(p).convert('RGB') if os.path.exists(p) and Image.open(p).size == size else None
    return base[size]
for n in names:
    p = os.path.join(d, n + '.png')
    if not os.path.exists(p): print('없음', n); continue
    im = Image.open(p).convert('RGB')
    b = base_for(im.size)
    box = None
    if b is not None and n != 'main':
        # 위 막대(시계가 바뀜) · 아래 시간표는 빼고 본다
        w, h = im.size
        diff = ImageChops.difference(im, b).convert('L').point(lambda v: 255 if v > 12 else 0)
        top = int(h * 0.085)
        diff.paste(0, (0, 0, w, top))
        box = diff.getbbox()
    if box:
        x0, y0, x1, y1 = box
        box = (max(0, x0 - 12), max(0, y0 - 12), min(im.size[0], x1 + 12), min(im.size[1], y1 + 12))
        im = im.crop(box)
    im.save(os.path.join(out, n + '.png'))
    print(n, im.size)
