# 창 캡처를 6장씩 묶어 훑어보기 (가운데 판 쪽만 잘라 줄임)
#   python tools/craft/grid.py <폴더> <접두> 이름 이름 ...
import sys, os
from PIL import Image
d, pre, *names = sys.argv[1:]
k = 0
for i in range(0, len(names), 6):
    ims = []
    for n in names[i:i + 6]:
        p = os.path.join(d, n + '.png')
        if not os.path.exists(p): continue
        im = Image.open(p).convert('RGB'); w, h = im.size
        ims.append(im.crop((int(w * .1375), int(h * .09), int(w * .8625), int(h * .89))).resize((870, 575)))
    sh = Image.new('RGB', (870 * 2 + 6, (575 + 6) * 3), 'white')
    for j, im in enumerate(ims): sh.paste(im, ((j % 2) * 876, (j // 2) * 581))
    sh.save(f'{pre}{k}.png'); print(k, names[i:i + 6]); k += 1
