# 캡처 여러 장을 줄여서 한 장으로 (훑어보기용)
#   python tools/craft/sheet.py <폴더> <출력.png> 이름 이름 ... [--w 1200]
import sys, os
from PIL import Image, ImageDraw
d, out, *names = sys.argv[1:]
w = 1000
if '--w' in names:
    i = names.index('--w'); w = int(names[i + 1]); del names[i:i + 2]
ims = []
for n in names:
    p = os.path.join(d, n + '.png')
    if not os.path.exists(p): continue
    im = Image.open(p).convert('RGB'); im = im.resize((w, int(im.size[1] * w / im.size[0])))
    ImageDraw.Draw(im).rectangle((0, 0, 200, 22), fill=(200, 0, 0)); ImageDraw.Draw(im).text((4, 4), n, fill=(255, 255, 255))
    ims.append(im)
cols = 2 if len(ims) > 1 else 1
rows = (len(ims) + cols - 1) // cols
H = max(i.size[1] for i in ims)
sheet = Image.new('RGB', (cols * w + (cols - 1) * 8, rows * H + (rows - 1) * 8), (255, 255, 255))
for k, im in enumerate(ims):
    sheet.paste(im, ((k % cols) * (w + 8), (k // cols) * (H + 8)))
sheet.save(out)
