// public/ 만들기: 저장소의 web/ 에서 정적 파일(자바스크립트 · CSS · 글꼴 · 그림)만 그대로 가져온다.
// - HTML 은 넣지 않는다 (페이지는 Worker 가 로그인을 본 뒤 src/pages.mjs 에서 어댑터를 끼워 보낸다).
// - 주소는 설치판 서버(server.rs · remote.rs)와 같게: /shared/* · /outputs/output.js · /outputs/stage.js · /remote.js · /groups.js · /i18n*.js
// - 웹판만의 파일: /cloud/adapter.js (client/adapter.js)
import { cpSync, mkdirSync, readdirSync, readFileSync, rmSync, statSync, copyFileSync, writeFileSync } from 'node:fs';
import { createHash } from 'node:crypto';
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
// 3단계 웹 조작 화면: tools/web/build.sh 가 만든 dist-web/ 의 wasm · js · 글꼴 · 견본 → /control/* (HTML 은 Worker 가 로그인을 본 뒤 보낸다)
// 정적 파일 하나 25 MiB 한도 (Workers 정적 파일) — 넘으면 멈춘다. dist-web 이 없으면(웹 조작 화면을 빌드하지 않은 CI) 건너뛴다
{
  const dist = join(cloud, '..', 'dist-web');
  const LIMIT = 25 * 1024 * 1024;
  try {
    const big = [];
    cpSync(dist, join(out, 'control'), { recursive: true, filter: src => {
      if (src.endsWith('.html') || src.endsWith('.br')) return false;
      const st = statSync(src);
      if (st.isFile() && st.size > LIMIT) big.push(`${relative(dist, src)} ${(st.size / 1048576).toFixed(1)} MiB`);
      return true;
    } });
    if (big.length) { console.error(`정적 파일 한도(25 MiB)를 넘는 파일: ${big.join(', ')}`); process.exit(1); }
  } catch (e) {
    if (e.code !== 'ENOENT') throw e;
    console.log('dist-web/ 이 없어 웹 조작 화면(/control)은 넣지 않음 — bash tools/web/build.sh');
  }
  // 주인 화면에 덧붙인 구획 (사람 · 교회 AI) → /cloud/control/*.js
  mkdirSync(join(out, 'cloud', 'control'), { recursive: true });
  for (const f of readdirSync(join(cloud, 'client', 'control'))) if (f.endsWith('.js')) copyFileSync(join(cloud, 'client', 'control', f), join(out, 'cloud', 'control', f));
}
// 4단계 B 미디어 부품 (client/media/*.js · *.mjs) + pdf.js 고정판 (package.json 의 pdfjs-dist, 예배 중 CDN 을 부르지 않게)
mkdirSync(join(out, 'cloud', 'media'), { recursive: true });
for (const f of readdirSync(join(cloud, 'client', 'media'))) {
  if (/\.(m?js|css)$/.test(f)) copyFileSync(join(cloud, 'client', 'media', f), join(out, 'cloud', 'media', f));
}
mkdirSync(join(out, 'cloud', 'vendor', 'pdfjs'), { recursive: true });
for (const f of ['pdf.min.mjs', 'pdf.worker.min.mjs']) copyFileSync(join(cloud, 'node_modules', 'pdfjs-dist', 'build', f), join(out, 'cloud', 'vendor', 'pdfjs', f));
copyFileSync(join(cloud, 'node_modules', 'pdfjs-dist', 'LICENSE'), join(out, 'cloud', 'vendor', 'pdfjs', 'LICENSE.txt'));
// 한글 PDF(CID 글꼴)에 필요한 CMap · 끼워 넣지 않은 기본 글꼴 · JPEG2000/JBIG2/색 프로필 풀이(wasm) 도 같은 곳에서
for (const d of ['cmaps', 'standard_fonts', 'wasm', 'iccs']) cpSync(join(cloud, 'node_modules', 'pdfjs-dist', d), join(out, 'cloud', 'vendor', 'pdfjs', d), { recursive: true });
// 정적 파일에도 보안 머리글 (Workers 정적 파일의 _headers 규칙)
// 6단계 오프라인 (client/offline/): 조작 화면 쪽 offline.js → /cloud/offline/*, 서비스 워커 → /sw.js (범위가 / 라야 출력 페이지까지 담는다)
// 판 이름 = 담을 파일 모두의 내용 지문 — 파일이 하나라도 바뀌면 새 판 (docs/web/offline.md)
mkdirSync(join(out, 'cloud', 'offline'), { recursive: true });
for (const f of readdirSync(join(cloud, 'client', 'offline'))) if (f.endsWith('.js') && f !== 'sw.js') copyFileSync(join(cloud, 'client', 'offline', f), join(out, 'cloud', 'offline', f));
{
  const list = [];
  const walk = d => { for (const f of readdirSync(d).sort()) { const p = join(d, f); if (statSync(p).isDirectory()) walk(p); else list.push(p); } };
  walk(out);
  const sw = readFileSync(join(cloud, 'client', 'offline', 'sw.js'), 'utf8');
  const h = createHash('sha256').update(sw);
  const urls = [];
  for (const p of list) {
    const u = '/' + relative(out, p).split(/[\\/]/).join('/');
    if (u.startsWith('/control/sample/')) continue; // 견본(로그인 없는 시험용)은 담지 않는다
    urls.push(u);
    h.update(u).update(readFileSync(p));
  }
  const version = h.digest('hex').slice(0, 12);
  writeFileSync(join(out, 'sw.js'), sw.replace("'__WP_VERSION__'", JSON.stringify(version)).replace('= __WP_PRECACHE__;', `= ${JSON.stringify(urls)};`));
  writeFileSync(join(out, 'sw-version.json'), JSON.stringify({ version, files: urls.length }) + '\n');
  console.log(`서비스 워커: 판 ${version} · 담을 파일 ${urls.length}개`);
}
// 정적 파일에도 보안 머리글 (Workers 정적 파일의 _headers 규칙). 서비스 워커는 늘 새로 묻게
writeFileSync(join(out, '_headers'), ['/*', '  X-Content-Type-Options: nosniff', '  Referrer-Policy: no-referrer', '/sw.js', '  Cache-Control: no-cache', ''].join('\n'));

let n = 0, bytes = 0;
const walk = d => { for (const f of readdirSync(d)) { const p = join(d, f); const s = statSync(p); if (s.isDirectory()) walk(p); else { n++; bytes += s.size; } } };
walk(out);
console.log(`public/: 파일 ${n}개 · ${(bytes / 1024 / 1024).toFixed(1)} MB (web/ 에서 가져옴: ${relative(cloud, web)})`);
