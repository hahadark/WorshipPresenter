"""웹 화면 전후 쌍 그림: tmp/craft/before/<이름>.png | tmp/craft/after/<이름>.png → tmp/craft/compare-<이름>.png
   python tools/webui/compare.py [--before tmp/craft/before] [--after tmp/craft/after] [--out tmp/craft] [이름 …]
   (같은 가짜 자료 · 같은 상태 · 같은 크기로 찍은 것 — tools/webui/shots.js)"""
import os
import sys
from PIL import Image, ImageDraw, ImageFont


def arg(k, d):
    return sys.argv[sys.argv.index(k) + 1] if k in sys.argv else d


before, after, out = arg('--before', 'tmp/craft/before'), arg('--after', 'tmp/craft/after'), arg('--out', 'tmp/craft')
skip = {arg('--before', ''), arg('--after', ''), arg('--out', '')}
names = [a for a in sys.argv[1:] if not a.startswith('--') and a not in skip]
if not names:
    names = sorted(f[:-4] for f in os.listdir(after) if f.endswith('.png') and os.path.exists(os.path.join(before, f)))
try:
    font = ImageFont.truetype('malgunbd.ttf', 18)
except OSError:
    font = ImageFont.load_default()
for n in names:
    a, b = Image.open(os.path.join(before, n + '.png')).convert('RGB'), Image.open(os.path.join(after, n + '.png')).convert('RGB')
    gap, head = 16, 34
    w, h = a.width + b.width + gap, max(a.height, b.height) + head
    o = Image.new('RGB', (w, h), (240, 240, 240))
    d = ImageDraw.Draw(o)
    d.text((8, 7), '전 (2.1.1)', fill=(60, 60, 60), font=font)
    d.text((a.width + gap + 8, 7), '후 (craft-web)', fill=(60, 60, 60), font=font)
    o.paste(a, (0, head))
    o.paste(b, (a.width + gap, head))
    p = os.path.join(out, 'compare-' + n + '.png')
    o.save(p)
    print(p)
