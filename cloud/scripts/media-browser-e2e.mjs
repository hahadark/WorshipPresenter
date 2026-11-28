// 4단계 B 미디어 — 브라우저 끝에서 끝 (헤드리스 엣지 + 크롬 가짜 카메라 · 가짜 설치판 · 로컬 클라우드). 화면 그림을 남긴다.
//   node scripts/media-browser-e2e.mjs [--out artifacts/media-browser] [--no-youtube]
// A 카메라를 이름으로 (두 탭이 같은 카메라를 함께)  B 휴대폰 카메라(WebRTC, 신호는 교회 중계실)  C PDF → 쪽 그림
// D 영상: 두 출력 탭 위치 맞추기 · 앞뒤로 넘기기(Range)  E 유튜브 임베드
// 실제 카메라는 쓰지 않는다 (이 PC 의 OBS Virtual Camera 는 화면 내용을 내보낼 수 있음 — CLAUDE.md). 영상은 ffmpeg 로 만든 시험 화면.
import { spawn, execFileSync } from 'node:child_process';
import { existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { createServer } from 'node:net';
import { createRequire } from 'node:module';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const cloud = join(dirname(fileURLToPath(import.meta.url)), '..');
const repo = join(cloud, '..');
const require = createRequire(import.meta.url);
const cdp = require(join(repo, 'tools', 'flow-compare', 'cdp.js'));
const args = process.argv.slice(2);
const out = resolve(args.includes('--out') ? args[args.indexOf('--out') + 1] : join(cloud, 'artifacts', 'media-browser'));
const wrangler = join(cloud, 'node_modules', 'wrangler', 'bin', 'wrangler.js');
process.env.WRANGLER_SEND_METRICS = 'false';
const sleep = ms => new Promise(r => setTimeout(r, ms));
const freePort = () => new Promise(res => { const s = createServer(); s.listen(0, '127.0.0.1', () => { const p = s.address().port; s.close(() => res(p)); }); });
const results = [];
const check = (name, ok, detail = '') => { results.push({ name, ok: !!ok, detail: String(detail) }); console.log(ok ? '  ✔' : '  ✖', name, detail); };
const kill = p => { if (p && p.exitCode === null) try { execFileSync('taskkill', ['/pid', String(p.pid), '/t', '/f'], { stdio: 'ignore' }); } catch { /* 꺼짐 */ } };
const TEMPLATE = JSON.parse(readFileSync(join(repo, 'crates', 'wp-core', 'data', 'template-base.json'), 'utf8'));
const payload = (extra) => ({ output: 'main', version: Date.now() % 1e9, hasLive: true, visible: true, template: TEMPLATE, cursorHidden: true, ...extra });

/** 시험용 PDF 두 쪽 (손으로 만든 PDF 1.4 — 글자 'Sample page N' 과 색 상자) */
function makePdf() {
  const objs = [];
  const add = s => { objs.push(s); return objs.length; };
  const font = add('<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>');
  const pages = [];
  const pagesId = objs.length + 1 + 4; // 아래에서 쪽 2개(내용 · 쪽) 뒤
  for (const [n, rgb] of [[1, '0.1 0.3 0.7'], [2, '0.7 0.2 0.1']]) {
    const content = `${rgb} rg 50 50 512 200 re f 0 0 0 rg BT /F1 48 Tf 72 400 Td (Sample page ${n}) Tj ET`;
    const c = add(`<< /Length ${content.length} >>\nstream\n${content}\nendstream`);
    pages.push(add(`<< /Type /Page /Parent ${pagesId} 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 ${font} 0 R >> >> /Contents ${c} 0 R >>`));
  }
  const ps = add(`<< /Type /Pages /Kids [${pages.map(p => `${p} 0 R`).join(' ')}] /Count ${pages.length} >>`);
  if (ps !== pagesId) throw new Error('pdf 번호');
  const cat = add(`<< /Type /Catalog /Pages ${ps} 0 R >>`);
  let s = '%PDF-1.4\n';
  const offs = [];
  objs.forEach((o, i) => { offs.push(s.length); s += `${i + 1} 0 obj\n${o}\nendobj\n`; });
  const x = s.length;
  s += `xref\n0 ${objs.length + 1}\n0000000000 65535 f \n${offs.map(o => String(o).padStart(10, '0') + ' 00000 n \n').join('')}trailer\n<< /Size ${objs.length + 1} /Root ${cat} 0 R >>\nstartxref\n${x}\n%%EOF\n`;
  return Buffer.from(s, 'latin1');
}

async function main() {
  rmSync(out, { recursive: true, force: true });
  mkdirSync(out, { recursive: true });
  if (!existsSync(join(cloud, '.secrets', 'sample.json'))) execFileSync(process.execPath, [join(cloud, 'scripts', 'seed.mjs')], { cwd: cloud, stdio: 'inherit' });
  execFileSync(process.execPath, [join(cloud, 'scripts', 'assets.mjs')], { cwd: cloud, stdio: 'inherit' });
  execFileSync(process.execPath, [wrangler, 'd1', 'execute', 'worshippresenter-web', '--local', '--command', 'DELETE FROM auth_failures'], { cwd: cloud, stdio: 'ignore' });
  // 시험 영상: 8초 · 640x360 · 화면에 시간이 보이는 testsrc (VP8 webm — 헤드리스 엣지가 확실히 재생)
  const video = join(out, 'sync.webm');
  execFileSync('ffmpeg', ['-y', '-loglevel', 'error', '-f', 'lavfi', '-i', 'testsrc=size=640x360:rate=30:duration=8', '-c:v', 'libvpx', '-b:v', '400k', '-g', '30', video]);
  const vbytes = readFileSync(video);

  const port = await freePort();
  const base = `http://127.0.0.1:${port}`;
  process.env.WP_BASE = base;
  const dev = spawn(process.execPath, [wrangler, 'dev', '--ip', '127.0.0.1', '--port', String(port), '--inspector-port', String(await freePort()), '--local', '--show-interactive-dev-session=false'], { cwd: cloud, stdio: 'ignore' });
  let browser = null, bridge = null;
  try {
    for (let t0 = Date.now(); ; ) { try { if ((await fetch(base + '/health')).ok) break; } catch { /* 아직 */ } if (Date.now() - t0 > 90000) throw new Error('wrangler dev'); await sleep(300); }
    const H = await import('../test/e2e/helpers.mjs');
    const S = H.SAMPLE, C = r => S.credentials[r];
    // 가짜 설치판: 미디어 요청(/media/sync.webm)은 Range 대로 답한다 (1단계 중계 길 — 2C 의 /api/v1/assets/{id} 도 같은 Range 규칙)
    bridge = await H.fakeBridge({ answer: () => null });
    let rangeHits = 0;
    bridge.ws.on('message', (data, isBinary) => {
      if (isBinary) return;
      let m; try { m = JSON.parse(data.toString()); } catch { return; }
      if (m.t !== 'req') return;
      if (m.path !== '/media/sync.webm') { bridge.send({ t: 'res', id: m.id, status: 404, body: { ok: false } }); return; }
      const size = vbytes.length;
      const rg = /bytes=(\d*)-(\d*)/.exec((m.headers && m.headers.range) || '');
      let a = 0, b = size - 1, status = 200;
      if (rg) { rangeHits++; status = 206; if (rg[1]) a = +rg[1]; if (rg[2]) b = Math.min(+rg[2], size - 1); if (!rg[1] && rg[2]) { a = size - +rg[2]; b = size - 1; } }
      const body = vbytes.subarray(a, b + 1);
      const headers = { 'content-type': 'video/webm', 'content-length': String(body.length), 'accept-ranges': 'bytes', ...(status === 206 ? { 'content-range': `bytes ${a}-${b}/${size}` } : {}) };
      bridge.send({ t: 'res', id: m.id, status, headers, stream: true });
      for (let o = 0; o < body.length; o += 200 * 1024) {
        const part = body.subarray(o, Math.min(o + 200 * 1024, body.length));
        const head = Buffer.alloc(5); head.writeUInt32BE(m.id, 0); head[4] = o + part.length >= body.length ? 1 : 0;
        bridge.ws.send(Buffer.concat([head, part]));
      }
      if (!body.length) { const head = Buffer.alloc(5); head.writeUInt32BE(m.id, 0); head[4] = 1; bridge.ws.send(head); }
    });
    const render = data => bridge.send({ t: 'render', client: 'main', data: payload(data) });

    browser = await cdp.launch(join(out, 'edge-profile'), undefined, ['--use-fake-device-for-media-stream', '--use-fake-ui-for-media-stream']);
    const outUrl = `${base}/outputs/main?key=${encodeURIComponent(C('output').key)}`;

    // ---------- A 카메라를 이름으로 ----------
    render({ slide: { kind: 'camera', camera: { deviceId: 'desktop-cam-1', fit: 'cover' } }, cameras: { 'desktop-cam-1': { label: 'fake_device_0' } } });
    const a1 = await browser.open(outUrl, 960, 540);
    const a2 = await browser.open(outUrl, 960, 540);
    const camOk = p => p.waitFor(`(() => { const v = document.querySelector('video[data-cam]'); return !!v && v.videoWidth > 0 && !v.paused; })()`, 20000);
    const [ok1, ok2] = [await camOk(a1), await camOk(a2)];
    check('A 카메라를 이름으로: 설치판 장치 id 대신 이 PC 의 같은 이름 장치를 연다', ok1, await a1.eval('JSON.stringify(window.WPMedia && WPMedia.resolved())'));
    const local = await a1.eval(`window.WPMedia.resolved()['desktop-cam-1'] || ''`);
    check('A 맞춘 장치 id 는 설치판 id 와 다르다 (PC · 사이트마다 다름)', local && local !== 'desktop-cam-1', local.slice(0, 12));
    check('A 같은 브라우저의 두 출력 탭이 같은 카메라를 함께 연다', ok1 && ok2);
    await a1.settle(); await a1.shot(join(out, 'A-camera-by-name.png'));
    // 이름이 다를 때: 이 PC 에서 짝을 정함 (/cameras 와 같은 저장소)
    render({ slide: { kind: 'camera', camera: { deviceId: 'desktop-cam-2', fit: 'cover' } }, cameras: { 'desktop-cam-2': { label: '강단 카메라' } } });
    await a1.eval(`localStorage.setItem('wp-cloud-camera-map', JSON.stringify({ '강단 카메라': 'fake_device_0' })), 1`);
    const ok3 = await a1.waitFor(`(() => { const v = document.querySelector('video[data-cam="desktop-cam-2"]'); return !!v && v.videoWidth > 0; })()`, 20000);
    check('A 이름이 다르면 /cameras 에서 정한 짝으로', ok3);
    await a2.close();

    // ---------- B 휴대폰 카메라 ----------
    const owner = await H.login('owner');
    const lk = await H.req('/api/v1/media/phone-link', { method: 'POST', cookie: owner, headers: { 'x-worshipcue': '1', origin: base }, body: { name: '시험 휴대폰' } });
    check('B 휴대폰 링크 만들기 (주인)', lk.status === 200, lk.status);
    const phone = await browser.open(lk.json.url, 420, 860);
    const sent = await phone.waitFor(`document.getElementById('st') && /연결됨|보내는 중/.test(document.getElementById('st').textContent)`, 20000);
    check('B 휴대폰 페이지(설치판 것 그대로)가 클라우드에 붙음', sent, await phone.eval(`document.getElementById('st').textContent`));
    const pid = await phone.eval(`localStorage.getItem('wp-cam-id') || ''`);
    await phone.eval(`(document.getElementById('name').value = '시험 휴대폰', 1)`);
    render({ slide: { kind: 'camera', camera: { deviceId: `phone:${pid}`, fit: 'cover' } }, cameras: { [`phone:${pid}`]: { label: '시험 휴대폰' } } });
    const pv = await a1.waitFor(`(() => { const v = document.querySelector('video[data-cam^="phone:"]'); return !!v && v.videoWidth > 0; })()`, 30000);
    check('B 출력 화면이 휴대폰 영상을 받는다 (WebRTC · 신호는 교회 중계실 · 영상은 직접)', pv, await a1.eval(`(() => { const v = document.querySelector('video[data-cam^="phone:"]'); return v ? v.videoWidth + 'x' + v.videoHeight : 'none'; })()`));
    check('B 휴대폰 화면에 "보내는 중"', await phone.waitFor(`/보내는 중/.test(document.getElementById('st').textContent)`, 10000));
    await a1.settle(); await a1.shot(join(out, 'B-phone-output.png'));
    await phone.shot(join(out, 'B-phone-page.png'));
    // 설치판에 등록한 휴대폰(다른 id)도 이름으로
    render({ slide: { kind: 'camera', camera: { deviceId: 'phone:desktopKnown1', fit: 'cover' } }, cameras: { 'phone:desktopKnown1': { label: '시험 휴대폰' } } });
    check('B 설치판 쪽 id 로 와도 이름으로 같은 휴대폰', await a1.waitFor(`(() => { const v = document.querySelector('video[data-cam="phone:desktopKnown1"]'); return !!v && v.videoWidth > 0; })()`, 30000));
    await phone.close();

    // ---------- C PDF ----------
    const pdfPage = await browser.open(`${base}/login?next=/cameras`, 1280, 800);
    await pdfPage.eval(`(async () => { await fetch('/auth/login', { method: 'POST', headers: { 'content-type': 'application/json', 'x-worshipcue': '1' }, body: JSON.stringify({ church: ${JSON.stringify(S.church)}, secret: ${JSON.stringify(C('owner').secret)} }) }); location.href = '/cameras'; return 1; })()`);
    await pdfPage.waitFor(`location.pathname === '/cameras'`, 10000);
    await sleep(800);
    const pdfB64 = makePdf().toString('base64');
    const pr = await pdfPage.eval(`(async () => {
      const m = await import('/cloud/media/pdf.mjs');
      const bin = Uint8Array.from(atob(${JSON.stringify(pdfB64)}), c => c.charCodeAt(0));
      const r = await m.pdfToSlideshow(bin.buffer, { title: '견본 주보', width: 1280 }); // 기본 올리기 = 2C POST /api/v1/assets (로컬 R2)
      const blobs = await Promise.all(r.pages.map(p => fetch(p.url).then(x => { if (!x.ok || x.headers.get('content-type') !== 'image/png') throw new Error('받기 ' + x.status); return x.blob(); })));
      // 그림이 비지 않았나 (색 상자 자리의 픽셀)
      const bmp = await createImageBitmap(blobs[0]);
      const cv = new OffscreenCanvas(bmp.width, bmp.height); const cx = cv.getContext('2d'); cx.drawImage(bmp, 0, 0);
      const px = cx.getImageData(Math.round(bmp.width * 0.3), Math.round(bmp.height * 0.8), 1, 1).data;
      document.body.innerHTML = r.pages.map(p => '<img src="' + p.url + '" style="width:600px;margin:8px;border:1px solid #888">').join('');
      return JSON.stringify({ item: r.item.type, pdf: r.item.pdf, n: r.pages.length, w: r.pages[0].width, h: r.pages[0].height, px: [...px], urls: r.item.images });
    })()`, 60000);
    const pj = JSON.parse(pr);
    check('C PDF → 쪽 그림 2장 (pdf.js 고정판, 이 사이트에서)', pj.n === 2 && pj.w === 1280 && pj.item === 'slideshow' && pj.pdf === true, `${pj.n}쪽 ${pj.w}x${pj.h}`);
    check('C 그린 쪽에 내용이 있다 (파란 상자)', pj.px[2] > 120 && pj.px[0] < 80, pj.px.join(','));
    check('C 쪽 그림을 R2(2C)에 올려 /api/v1/assets/{id} 로 순서에 넣음', pj.urls.every(u => /^\/api\/v1\/assets\/[\w-]+$/.test(u)), pj.urls.join(' '));
    await sleep(500);
    await pdfPage.shot(join(out, 'C-pdf-pages.png'));

    // ---------- D 영상: R2(2C)에 올린 영상 · 두 탭 위치 맞추기 · 넘기기 ----------
    const { createHash } = await import('node:crypto');
    const up = await fetch(`${base}/api/v1/assets`, { method: 'POST', headers: { cookie: owner, origin: base, 'x-worshipcue': '1', 'content-type': 'video/webm', 'x-asset-sha256': createHash('sha256').update(vbytes).digest('hex'), 'x-asset-name': encodeURIComponent('시험 영상.webm') }, body: vbytes }).then(r => r.json());
    check('D 시험 영상을 R2(2C, 로컬)에 올림', up.ok && up.id, up.id || up.error);
    const vurl = `/api/v1/assets/${up.id}?fallback=${encodeURIComponent('/media/sync.webm')}`;
    const rg = await fetch(base + vurl, { headers: { cookie: owner, range: 'bytes=1000-1999' } });
    check('D R2 영상 Range 206', rg.status === 206 && (await rg.arrayBuffer()).byteLength === 1000, rg.headers.get('content-range'));
    const t0 = Date.now();
    render({ slide: { kind: 'video', image: vurl, video: { sound: false, loop: true } }, video: { key: 'v1', playing: true, offset: 0, at: t0, now: t0, loop: true } });
    const d2 = await browser.open(outUrl, 960, 540);
    const playing = p => p.waitFor(`(() => { const v = document.querySelector('video.bc-play'); return !!v && v.readyState >= 2 && !v.paused && v.currentTime > 0.3; })()`, 30000);
    const [pa, pb] = [await playing(a1), await playing(d2)];
    check('D 클라우드 출력 두 탭이 R2 영상을 재생', pa && pb);
    await sleep(4000);
    const diffs = await Promise.all([a1, d2].map(p => p.eval(`JSON.stringify({ lead: WPMediaSync.lead(), last: WPMediaSync.last() })`).then(JSON.parse)));
    const follower = diffs.find(d => !d.lead);
    check('D 같은 PC 의 탭끼리: 한 탭이 기준, 다른 탭의 차이가 0.1초 아래', follower && follower.last && Math.abs(follower.last.diff) < 0.1, follower && follower.last ? `차이 ${(follower.last.diff * 1000).toFixed(0)}ms` : 'none');
    // 앞뒤로 넘기기: 설치판이 위치를 6초로 (영상 조작 seek)
    const t1 = Date.now();
    render({ slide: { kind: 'video', image: vurl, video: { sound: false, loop: true } }, video: { key: 'v1', playing: false, offset: 6, at: t1, now: t1, loop: true } });
    const seekOk = p => p.waitFor(`(() => { const v = document.querySelector('video.bc-play'); return !!v && v.paused && !v.seeking && v.readyState >= 2 && Math.abs(v.currentTime - 6) < 0.6; })()`, 15000); // currentTime 은 넘기기를 시작하자마자 바뀌므로 다 넘겨졌는지(seeking 끝 · 그림 있음)까지
    check('D 넘기기: 두 탭 모두 6초 자리에서 멈춤 (Range 로 그 자리만 받음)', (await seekOk(a1)) && (await seekOk(d2)));
    // currentTime 만으로는 넘기기가 끝났는지 모른다 → 영상 요소가 실제로 그린 그림을 ffmpeg 기준 그림(초마다)과 맞춰 본다
    const refs = {};
    for (const sec of [2, 3, 4, 5, 6, 7]) refs[sec] = 'data:image/png;base64,' + execFileSync('ffmpeg', ['-loglevel', 'error', '-ss', String(sec + 0.2), '-i', video, '-frames:v', '1', '-vf', 'scale=320:180', '-f', 'image2', '-c:v', 'png', 'pipe:1']).toString('base64');
    const shown = async p => p.eval(`(async () => {
      const refs = ${JSON.stringify(refs)};
      const grab = async src => { const c = new OffscreenCanvas(320, 180), x = c.getContext('2d'); if (typeof src === 'string') { const i = new Image(); i.src = src; await i.decode(); x.drawImage(i, 0, 0, 320, 180); } else x.drawImage(src, 0, 0, 320, 180); return x.getImageData(250, 50, 45, 45).data; };
      const v = await grab(document.querySelector('video.bc-play'));
      let best = null, bestD = 1e18;
      for (const [sec, src] of Object.entries(refs)) { const r = await grab(src); let d = 0; for (let i = 0; i < r.length; i += 4) d += Math.abs(r[i] - v[i]) + Math.abs(r[i + 1] - v[i + 1]) + Math.abs(r[i + 2] - v[i + 2]); if (d < bestD) { bestD = d; best = +sec; } }
      return best;
    })()`);
    const [s1, s2] = [await shown(a1), await shown(d2)];
    check('D 두 탭의 영상 요소가 실제로 6초 자리 그림을 그림 (ffmpeg 기준 그림과 대조)', s1 === 6 && s2 === 6, `탭1 ${s1}초 · 탭2 ${s2}초 그림`);
    await a1.settle(); await a1.shot(join(out, 'D-video-tab1.png'));
    await d2.settle(); await d2.shot(join(out, 'D-video-tab2.png'));
    await d2.close();
    // 1단계 길(설치판 중계)도 같은 페이지 코드로: R2 에 없을 때 쓰는 fallback 주소
    const t3 = Date.now();
    render({ slide: { kind: 'video', image: '/media/sync.webm', video: { sound: false, loop: true } }, video: { key: 'v2', playing: true, offset: 2, at: t3, now: t3, loop: true } });
    check('D 설치판 중계 길(/media/…)의 영상도 재생 · 넘기기 (Range)', await playing(a1), `중계 Range 요청 ${rangeHits}번`);

    // ---------- E 유튜브 ----------
    if (!args.includes('--no-youtube')) {
      const t2 = Date.now();
      render({ slide: { kind: 'video', youtube: { id: 'M7lc1UVf-VE', start: 0 }, video: { sound: false } }, video: { key: 'yt1', playing: true, offset: 0, at: t2, now: t2 } });
      const yf = await a1.waitFor(`(() => { const f = document.querySelector('iframe.bc-yt'); return !!f && f.getAttribute('referrerpolicy') === 'strict-origin-when-cross-origin' && f.src.includes('youtube-nocookie.com/embed/M7lc1UVf-VE'); })()`, 15000);
      check('E 유튜브: 클라우드 출력에서 같은 임베드 (referrerpolicy 를 iframe 에 직접 — 페이지의 no-referrer 와 상관없음)', yf);
      await sleep(8000);
      await a1.shot(join(out, 'E-youtube.png'));
    }
  } finally {
    if (bridge) try { await bridge.close(); } catch { /* 무시 */ }
    if (browser) try { browser.close(); } catch { /* 무시 */ }
    kill(dev);
  }
  writeFileSync(join(out, 'result.json'), JSON.stringify(results, null, 2));
  const bad = results.filter(r => !r.ok);
  console.log(`\n${results.length - bad.length}/${results.length} 통과 → ${out}`);
  process.exit(bad.length ? 1 : 0);
}
main().catch(e => { console.error(e); process.exit(1); });
