// 배포한 미리보기에 대고: 파일 올리기 · 받기(R2) · 권한 · 미디어 페이지 · PDF 한 개 → 끝나면 지우고 정리(R2 에서 실제로 지움).
//   node scripts/preview-check.mjs --base https://<미리보기> [--sample sample.remote.json]
// 견본 파일만 (ffmpeg 시험 영상 · 손으로 만든 PNG · PDF). 실제 자료 없음. 비밀값은 .secrets/ 에서 읽고 출력하지 않는다.
import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { mkdirSync, readFileSync, rmSync } from 'node:fs';
import { createRequire } from 'node:module';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const cloud = join(dirname(fileURLToPath(import.meta.url)), '..');
const require = createRequire(import.meta.url);
const cdp = require(join(cloud, '..', 'tools', 'flow-compare', 'cdp.js'));
const args = process.argv.slice(2);
const opt = (k, d) => { const i = args.indexOf(k); return i >= 0 ? args[i + 1] : d; };
const base = opt('--base', '');
if (!/^https:\/\//.test(base)) throw new Error('--base https://… 가 필요합니다');
const S = JSON.parse(readFileSync(join(cloud, '.secrets', opt('--sample', 'sample.remote.json')), 'utf8'));
const C = r => S.credentials[r];
const results = [];
const check = (name, ok, detail = '') => { results.push({ name, ok: !!ok }); console.log(ok ? '  ✔' : '  ✖', name, detail); };
const W = { 'x-worshipcue': '1', origin: base };
const sha = b => createHash('sha256').update(b).digest('hex');

async function loginCookie(secret) {
  const r = await fetch(`${base}/auth/login`, { method: 'POST', headers: { ...W, 'content-type': 'application/json' }, body: JSON.stringify({ church: S.church, secret }) });
  if (!r.ok) throw new Error(`로그인 ${r.status}`);
  return r.headers.getSetCookie().map(c => c.split(';')[0]).join('; ');
}
async function upload(cookie, bytes, type, name) {
  const r = await fetch(`${base}/api/v1/assets`, { method: 'POST', headers: { ...W, cookie, 'content-type': type, 'x-asset-sha256': sha(bytes), 'x-asset-name': encodeURIComponent(name) }, body: bytes });
  return { status: r.status, json: await r.json().catch(() => null) };
}

/** 1x1 PNG 를 조금씩 다르게 (실행마다 새 sha256) */
function png(seed) {
  const b = Buffer.from('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==', 'base64');
  return Buffer.concat([b, Buffer.from(`wp-preview-check ${seed}`)]); // PNG 끝 뒤에 붙인 글은 그림에 영향 없음
}

async function main() {
  const owner = await loginCookie(C('owner').secret);
  const made = [];
  const run = Date.now().toString(36);
  // ---------- 파일 올리기 · 받기 ----------
  const p = png(run);
  const u1 = await upload(owner, p, 'image/png', `C:\\Users\\누구\\견본-${run}.png`);
  check('그림 올리기 (R2, sha256 확인)', u1.status === 201 && u1.json.ok && u1.json.asset.name === `견본-${run}.png`, `${u1.status} ${u1.json && u1.json.id}`);
  if (u1.json && u1.json.id) made.push(u1.json.id);
  const again = await upload(owner, p, 'image/png', 'x.png');
  check('같은 파일 다시 올리면 있던 것 (existed)', again.json && again.json.existed === true && again.json.id === u1.json.id);
  const bad = await fetch(`${base}/api/v1/assets`, { method: 'POST', headers: { ...W, cookie: owner, 'content-type': 'image/png', 'x-asset-sha256': sha(Buffer.from('other')), 'x-asset-name': 'y.png' }, body: png(run + 'x') }).then(r => r.status);
  check('sha256 이 다르면 거절 (R2 가 확인)', bad === 400, bad);
  const dir = join(cloud, 'artifacts', 'preview-check');
  rmSync(dir, { recursive: true, force: true }); mkdirSync(dir, { recursive: true });
  const vfile = join(dir, 'sample.webm');
  execFileSync('ffmpeg', ['-y', '-loglevel', 'error', '-f', 'lavfi', '-i', `testsrc=size=320x180:rate=15:duration=3`, '-c:v', 'libvpx', '-b:v', '200k', '-metadata', `title=${run}`, vfile]);
  const vb = readFileSync(vfile);
  const u2 = await upload(owner, vb, 'video/webm', `견본 영상 ${run}.webm`);
  check('영상 올리기', u2.status === 201, `${u2.status} ${vb.length}B`);
  if (u2.json && u2.json.id) made.push(u2.json.id);
  const full = await fetch(`${base}/api/v1/assets/${u2.json.id}`, { headers: { cookie: owner } });
  const fb = Buffer.from(await full.arrayBuffer());
  check('받기: 올린 것과 같은 바이트', full.status === 200 && sha(fb) === sha(vb), `${full.status} ${full.headers.get('content-type')}`);
  const rg = await fetch(`${base}/api/v1/assets/${u2.json.id}`, { headers: { cookie: owner, range: 'bytes=100-199' } });
  const rb = Buffer.from(await rg.arrayBuffer());
  check('받기: Range 206 (영상 넘기기)', rg.status === 206 && rb.equals(vb.subarray(100, 200)), rg.headers.get('content-range'));
  const etag = full.headers.get('etag');
  check('받기: ETag 가 맞으면 304', (await fetch(`${base}/api/v1/assets/${u2.json.id}`, { headers: { cookie: owner, 'if-none-match': etag } })).status === 304);
  check('로그인 없이 받기는 401', (await fetch(`${base}/api/v1/assets/${u2.json.id}`)).status === 401);
  const outRes = await fetch(`${base}/outputs/main?key=${encodeURIComponent(C('output').key)}`, { redirect: 'manual' });
  const outCookie = outRes.headers.getSetCookie().map(c => c.split(';')[0]).join('; ');
  check('출력 열쇠(OBS)로도 자기 교회 파일은 받음', (await fetch(`${base}/api/v1/assets/${u1.json.id}`, { headers: { cookie: outCookie } })).status === 200);
  check('출력 열쇠로는 올리기 403', (await upload(outCookie, png(run + 'o'), 'image/png', 'o.png')).status === 403);

  // ---------- 미디어 페이지 ----------
  check('/cameras (카메라 맞추기) 열림', (await fetch(`${base}/cameras`, { headers: { cookie: owner } })).status === 200);
  check('출력 페이지에 미디어 부품', /\/cloud\/media\/media\.js/.test(await fetch(`${base}/outputs/main`, { headers: { cookie: outCookie } }).then(r => r.text())));
  const lk = await fetch(`${base}/api/v1/media/phone-link`, { method: 'POST', headers: { ...W, cookie: owner, 'content-type': 'application/json' }, body: '{}' }).then(r => r.json());
  check('휴대폰 링크 · 휴대폰 페이지', lk.ok && (await fetch(lk.url)).status === 200);
  check('pdf.js 고정판이 이 사이트에 (CDN 아님)', (await fetch(`${base}/cloud/vendor/pdfjs/pdf.min.mjs`)).status === 200 && (await fetch(`${base}/cloud/vendor/pdfjs/cmaps/KSC-EUC-H.bcmap`)).status === 200);

  // ---------- PDF 한 개 (헤드리스 엣지, 실제 R2 에 쪽 그림 올림) ----------
  const browser = await cdp.launch(join(dir, 'edge-profile'));
  try {
    const page = await browser.open(`${base}/login?next=/cameras`, 1200, 800);
    await page.eval(`(async () => { await fetch('/auth/login', { method: 'POST', headers: { 'content-type': 'application/json', 'x-worshipcue': '1' }, body: JSON.stringify({ church: ${JSON.stringify(S.church)}, secret: ${JSON.stringify(C('owner').secret)} }) }); location.href = '/cameras'; return 1; })()`);
    await page.waitFor(`location.pathname === '/cameras'`, 15000);
    const pdf = makePdf(run);
    const r = JSON.parse(await page.eval(`(async () => {
      const m = await import('/cloud/media/pdf.mjs');
      const bin = Uint8Array.from(atob(${JSON.stringify(pdf.toString('base64'))}), c => c.charCodeAt(0));
      const r = await m.pdfToSlideshow(bin.buffer, { title: '견본 PDF ${run}', width: 1280 });
      const ok = await Promise.all(r.item.images.map(u => fetch(u).then(x => x.status + ' ' + x.headers.get('content-type'))));
      document.body.innerHTML = r.item.images.map(u => '<img src="' + u + '" style="width:540px;margin:6px;border:1px solid #888">').join('');
      return JSON.stringify({ item: r.item, ok, ids: r.item.images.map(u => u.split('/').pop()) });
    })()`, 90000));
    check('PDF → 쪽 그림 2장을 R2 에 올리고 슬라이드쇼 순서 (설치판과 같은 모양)', r.item.type === 'slideshow' && r.item.pdf === true && r.item.images.length === 2 && r.ok.every(x => x === '200 image/png'), r.ok.join(', '));
    made.push(...r.ids);
    await new Promise(res => setTimeout(res, 1500));
    await page.shot(join(dir, 'preview-pdf.png'));
  } finally { try { browser.close(); } catch { /* 무시 */ } }

  // ---------- 지우기 · 정리 (R2 에서 실제로) ----------
  for (const id of made) await fetch(`${base}/api/v1/assets/${id}`, { method: 'DELETE', headers: { ...W, cookie: owner } });
  const left = await fetch(`${base}/api/v1/assets?limit=200`, { headers: { cookie: owner } }).then(r => r.json());
  check('지운 표시 뒤 목록에서 사라짐', !left.assets.some(a => made.includes(a.id)), `${made.length}개`);
  const purge = await fetch(`${base}/api/v1/assets/purge`, { method: 'POST', headers: { ...W, cookie: owner, 'content-type': 'application/json' }, body: JSON.stringify({ olderThanDays: 0, dryRun: false, confirm: 'now' }) }).then(r => r.json());
  check('정리: R2 에서 실제로 지움', purge.ok, JSON.stringify(purge).slice(0, 160));
  const gone = await fetch(`${base}/api/v1/assets/${made[0]}`, { headers: { cookie: owner } });
  check('정리한 파일은 410', gone.status === 410, gone.status);
  const usage = await fetch(`${base}/api/v1/assets/usage`, { headers: { cookie: owner } }).then(r => r.json());
  console.log('  · 사용량', JSON.stringify(usage).slice(0, 200));
  const bad2 = results.filter(x => !x.ok);
  console.log(`\n${results.length - bad2.length}/${results.length} 통과`);
  process.exit(bad2.length ? 1 : 0);
}

function makePdf(tag) {
  const objs = [];
  const add = s => { objs.push(s); return objs.length; };
  const font = add('<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>');
  const kids = [];
  const pagesId = 6;
  for (const [n, rgb] of [[1, '0.1 0.3 0.7'], [2, '0.7 0.2 0.1']]) {
    const content = `${rgb} rg 50 50 512 200 re f 0 0 0 rg BT /F1 36 Tf 72 400 Td (Preview check ${tag} page ${n}) Tj ET`;
    const c = add(`<< /Length ${content.length} >>\nstream\n${content}\nendstream`);
    kids.push(add(`<< /Type /Page /Parent ${pagesId} 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 ${font} 0 R >> >> /Contents ${c} 0 R >>`));
  }
  add(`<< /Type /Pages /Kids [${kids.map(k => `${k} 0 R`).join(' ')}] /Count 2 >>`);
  const cat = add(`<< /Type /Catalog /Pages ${pagesId} 0 R >>`);
  let s = '%PDF-1.4\n';
  const offs = [];
  objs.forEach((o, i) => { offs.push(s.length); s += `${i + 1} 0 obj\n${o}\nendobj\n`; });
  const x = s.length;
  s += `xref\n0 ${objs.length + 1}\n0000000000 65535 f \n${offs.map(o => String(o).padStart(10, '0') + ' 00000 n \n').join('')}trailer\n<< /Size ${objs.length + 1} /Root ${cat} 0 R >>\nstartxref\n${x}\n%%EOF\n`;
  return Buffer.from(s, 'latin1');
}
main().catch(e => { console.error(e); process.exit(1); });
