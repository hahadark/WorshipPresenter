// public/ 만들기: 저장소의 web/ 에서 정적 파일(자바스크립트 · CSS · 글꼴 · 그림)만 그대로 가져온다.
// - HTML 은 넣지 않는다 (페이지는 Worker 가 로그인을 본 뒤 src/pages.mjs 에서 어댑터를 끼워 보낸다).
// - 주소는 설치판 서버(server.rs · remote.rs)와 같게: /shared/* · /outputs/output.js · /outputs/stage.js · /remote.js · /groups.js · /i18n*.js
// - 웹판만의 파일: /cloud/adapter.js (client/adapter.js)
import { cpSync, mkdirSync, readdirSync, rmSync, statSync, copyFileSync, writeFileSync } from 'node:fs';
import { dirname, join, relative } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const cloud = join(here, '..');
const web = join(cloud, '..', 'web');
const out = join(cloud, 'public');

rmSync(out, { recursive: true, force: true });
mkdirSync(out, { recursive: true });

const skipHtml = src => !src.endsWith('.html');
cpSync(join(web, 'shared'), join(out, 'shared'), { recursive: true, filter: skipHtml });
const files = [
  ['output/output.js', 'outputs/output.js'],
  ['output/stage.js', 'outputs/stage.js'],
  ['remote/remote.js', 'remote.js'],
  ['shared/groups.js', 'groups.js'],
  ['shared/i18n.js', 'i18n.js'],
  ['shared/i18n-en.js', 'i18n-en.js'],
];
for (const [from, to] of files) {
  mkdirSync(dirname(join(out, to)), { recursive: true });
  copyFileSync(join(web, from), join(out, to));
}
mkdirSync(join(out, 'cloud'), { recursive: true });
for (const f of readdirSync(join(cloud, 'client'))) {
  if (f.endsWith('.js') || f.endsWith('.css')) copyFileSync(join(cloud, 'client', f), join(out, 'cloud', f));
}
// 정적 파일에도 보안 머리글 (Workers 정적 파일의 _headers 규칙)
writeFileSync(join(out, '_headers'), ['/*', '  X-Content-Type-Options: nosniff', '  Referrer-Policy: no-referrer', ''].join('\n'));

let n = 0, bytes = 0;
const walk = d => { for (const f of readdirSync(d)) { const p = join(d, f); const s = statSync(p); if (s.isDirectory()) walk(p); else { n++; bytes += s.size; } } };
walk(out);
console.log(`public/: 파일 ${n}개 · ${(bytes / 1024 / 1024).toFixed(1)} MB (web/ 에서 가져옴: ${relative(cloud, web)})`);
