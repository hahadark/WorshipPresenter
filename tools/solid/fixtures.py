"""Synthetic, isolated fixtures. Never opens an installed app or user data."""
import argparse
import json
from pathlib import Path
import re
import shutil
import subprocess
import sqlite3
import struct
import zlib

ROOT = Path(__file__).resolve().parents[2]


def png(path, width=1280, height=720, color=(45, 65, 75)):
    def chunk(kind, data):
        return struct.pack('>I', len(data)) + kind + data + struct.pack('>I', zlib.crc32(kind + data))
    raw = (b'\0' + bytes(color) * width) * height
    path.write_bytes(b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('>IIBBBBB', width, height, 8, 2, 0, 0, 0)) + chunk(b'IDAT', zlib.compress(raw)) + chunk(b'IEND', b''))


def create(out, large=False):
    out.mkdir(parents=True, exist_ok=True)
    seed = out / 'seed'
    seed.mkdir(exist_ok=True)
    media = seed / 'media'
    media.mkdir(exist_ok=True)
    png(media / 'sample.png')
    settings = json.loads((ROOT / 'crates/wp-core/data/defaults.json').read_text(encoding='utf-8'))
    settings.update(autoUpdate=False, telemetry=False, globalShortcuts=False, syncDir='', activeCueId='', versionId='solid', triggerKey='solid-local-tests', port=37980)
    settings['output'].update(autoOnSend=False, enabled=False)
    settings['remote'] = {'enabled': False}
    settings['lanOutput'] = {'enabled': False}
    settings['ui'] = {'theme': 'pro', 'scale': 100}
    settings['templatesFor'] = {'main': 'worship', 'overlay': 'overlay'}
    for t in settings['templates']:
        t['logo'] = {'show': False, 'image': ''}
    (seed / 'settings.json').write_text(json.dumps(settings, ensure_ascii=False, indent=1), encoding='utf-8')
    db = sqlite3.connect(seed / 'bible.db')
    schema = re.search(r'const SCHEMA: &str = r#"(.*?)"#;', (ROOT / 'crates/wp-core/src/db.rs').read_text(encoding='utf-8'), re.S)[1]
    db.executescript(schema)
    db.execute('INSERT OR IGNORE INTO BibleVersions(id,name,language,verse_count) VALUES(?,?,?,?)', ('solid', '공개 시험 본문', 'ko', 178))
    for book, chapter, verses in [(43, 3, range(16, 18)), (19, 119, range(1, 177))]:
        for verse in verses:
            text = '하나님이 세상을 이처럼 사랑하사 독생자를 주셨으니' if book == 43 else f'말씀을 따라 걸으며 주님께 감사하고 찬양합니다. 긴 본문 점검 {verse}절.'
            db.execute('INSERT OR IGNORE INTO BibleVerses(version_id,book_id,chapter,verse,text,search_text,search_compact) VALUES(?,?,?,?,?,?,?)', ('solid', book, chapter, verse, text, text, text.replace(' ', '')))
    db.execute("CREATE TABLE IF NOT EXISTS Songs (id TEXT PRIMARY KEY,title TEXT NOT NULL,lyrics TEXT NOT NULL DEFAULT '',credit TEXT NOT NULL DEFAULT '',tags TEXT NOT NULL DEFAULT '',updated_at TEXT)")
    if large:
        db.executemany('INSERT OR REPLACE INTO Songs(id,title,lyrics) VALUES(?,?,?)', [(f'solid{i:06}', f'시험 찬양 {i:04}', '[1절]\n주님을 찬양합니다\n온 마음 다하여\n\n[후렴]\n감사와 찬양을 드립니다') for i in range(5000)])
        library = seed / 'library'
        library.mkdir(exist_ok=True)
        for i in range(300):
            png(library / f'image-{i:03}.png', 640, 360, (i % 256, 70, 100))
        settings['libraryFolders'] = [str(library.resolve())]
        (seed / 'settings.json').write_text(json.dumps(settings, ensure_ascii=False, indent=1), encoding='utf-8')
    db.commit()
    db.close()
    items = [{'id': f'solid{i}', 'type': 'title', 'title': f'시험 순서 {i + 1}', 'subtitle': '공개 점검 자료'} for i in range(200 if large else 16)]
    items[1] = {'id': 'song1', 'type': 'song', 'title': '시험 찬양', 'lyrics': '[1절]\n감사하며 찬양합니다\n주님과 함께 걸어갑니다'}
    for i in (2, 5):
        items[i] = {'id': f'image{i}', 'type': 'image', 'title': '그림 안내', 'image': '/media/sample.png', 'caption': '함께 예배드리신 여러분을 환영합니다'}
    items[6] = {'id': 'bible6', 'type': 'bible', 'title': '성경봉독', 'ref': '요3:16-17'}
    items[7] = {'id': 'score7', 'type': 'slideshow', 'title': '악보', 'slides': [{'image': '/media/sample.png', 'caption': '온 마음 다하여 찬양합니다'}]}
    # 영상 (ffmpeg 가 있으면 1080p 시험 영상을 만든다 — 성능 점검의 '영상 재생 중')
    video = media / 'test.mp4'
    if not video.exists() and shutil.which('ffmpeg'):
        subprocess.run(['ffmpeg', '-v', 'error', '-y', '-f', 'lavfi', '-i', 'testsrc2=size=1920x1080:rate=30', '-t', '60', '-pix_fmt', 'yuv420p', '-c:v', 'libx264', '-preset', 'veryfast', str(video)], check=False)
    items[9] = {'id': 'ps119', 'type': 'bible', 'title': '시편 119편', 'ref': '시119'}
    items[8] = {'id': 'video8', 'type': 'video', 'title': '시험 영상', 'file': '/media/test.mp4', 'sound': False, 'volume': 0, 'endAction': 'loop'}
    (out / 'cue.json').write_text(json.dumps({'format': 'worshipcue-cues', 'version': 1, 'cues': [{'name': '점검 예배', 'items': items}]}, ensure_ascii=False), encoding='utf-8')
    (out / 'empty-legacy').mkdir(exist_ok=True)
    print(seed.resolve())


if __name__ == '__main__':
    p = argparse.ArgumentParser()
    p.add_argument('--out', type=Path, default=ROOT / 'tmp/solid/fixtures')
    p.add_argument('--large', action='store_true')
    args = p.parse_args()
    create(args.out, args.large)
