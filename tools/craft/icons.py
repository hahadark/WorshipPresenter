"""16px 원본 경로에서 독립 SVG 생성. python tools/craft/icons.py [--check]"""
import json
from pathlib import Path
import sys

root = Path(__file__).resolve().parents[2]
source = root / "assets/icons/craft.json"
target = root / "assets/icons/craft"
target.mkdir(parents=True, exist_ok=True)
icons = json.loads(source.read_text(encoding="utf-8"))
bad = []
for name, body in icons.items():
    svg = f'<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.25" stroke-linecap="round" stroke-linejoin="round">{body}</svg>\n'
    path = target / (name + ".svg")
    if "--check" in sys.argv:
        if not path.exists() or path.read_text(encoding="utf-8") != svg:
            bad.append(name)
    else:
        path.write_text(svg, encoding="utf-8")
if bad:
    raise SystemExit("SVG differs: " + ", ".join(bad))
print(f"{len(icons)} craft SVGs: {'verified' if '--check' in sys.argv else 'written'}")
