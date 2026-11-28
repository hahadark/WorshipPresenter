// 설치판 + 로컬 클라우드 끝에서 끝 (화면 그림을 남긴다)
//   node scripts/desktop-e2e.mjs --exe ..\target\debug\wp-app.exe [--out artifacts\desktop-e2e]
// 1) 빈 포트에 wrangler dev (로컬 D1 · DO) — 견본 교회 'sample'
// 2) 격리한 자료 폴더(빈 bible.db · 설정)로 설치판을 띄움: 클라우드 연결 켬 · 다른 작업자와 겹치지 않는 포트 · WP_LEGACY_DATA 는 빈 폴더
// 3) 클라우드 웹 편집기로 견본 큐시트를 만들고 → 설치판을 껐다 켬(끊김 · 다시 이음) → 클라우드 리모컨으로 송출 · 다음
// 4) 클라우드 출력 주소(예배당 · OBS 자막 · 무대)와 이 PC 의 로컬 출력 주소를 헤드리스 엣지로 찍어 나란히 둔다
// 실제 사용자 자료는 읽지도 쓰지도 않는다. 견본 글만 쓴다.
import { spawn, execFileSync } from 'node:child_process';
import { existsSync, mkdirSync, rmSync, writeFileSync, readFileSync } from 'node:fs';
import { createServer } from 'node:net';
import { createRequire } from 'node:module';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import WebSocket from 'ws';

const cloud = join(dirname(fileURLToPath(import.meta.url)), '..');
const repo = join(cloud, '..');
const require = createRequire(import.meta.url);
const cdp = require(join(repo, 'tools', 'flow-compare', 'cdp.js'));
const args = process.argv.slice(2);
const opt = (k, d) => { const i = args.indexOf(k); return i >= 0 ? args[i + 1] : d; };
const exe = resolve(opt('--exe', join(repo, 'target', 'debug', 'wp-app.exe')));
const out = resolve(opt('--out', join(cloud, 'artifacts', 'desktop-e2e')));
const wrangler = join(cloud, 'node_modules', 'wrangler', 'bin', 'wrangler.js');
process.env.WRANGLER_SEND_METRICS = 'false';
const sleep = ms => new Promise(r => setTimeout(r, ms));
const freePort = () => new Promise(res => { const s = createServer(); s.listen(0, '127.0.0.1', () => { const p = s.address().port; s.close(() => res(p)); }); });
const log = (...a) => console.log('·', ...a);
const results = [];
const check = (name, ok, detail = '') => { results.push({ name, ok: !!ok, detail }); console.log(ok ? '  ✔' : '  ✖', name, detail); };
const kill = p => { if (!p || p.exitCode !== null) return; try { execFileSync('taskkill', ['/pid', String(p.pid), '/t', '/f'], { stdio: 'ignore' }); } catch { /* 이미 꺼짐 */ } };

async function main() {
  if (!existsSync(exe)) throw new Error(`설치판 실행 파일이 없습니다: ${exe} (cargo build -p wp-app)`);
  rmSync(out, { recursive: true, force: true });
  mkdirSync(out, { recursive: true });
  if (!existsSync(join(cloud, '.secrets', 'sample.json'))) execFileSync(process.execPath, [join(cloud, 'scripts', 'seed.mjs')], { cwd: cloud, stdio: 'inherit' });
  execFileSync(process.execPath, [join(cloud, 'scripts', 'assets.mjs')], { cwd: cloud, stdio: 'inherit' });
  const given = opt('--base', '');
  if (!given) execFileSync(process.execPath, [wrangler, 'd1', 'execute', 'worshippresenter-web', '--local', '--command', 'DELETE FROM auth_failures'], { cwd: cloud, stdio: 'ignore' });
  // --base https://… --sample sample.remote.json → 배포한 미리보기에 대고 (설치판의 wss · TLS 길을 시험)
  const S = JSON.parse(readFileSync(join(cloud, '.secrets', opt('--sample', 'sample.json')), 'utf8'));
  const local = !given;
  const ipH = ip => (local ? { 'cf-connecting-ip': ip } : {});
  const C = r => S.credentials[r];

  // 1) 로컬 클라우드
  const port = await freePort();
  const base = given || `http://127.0.0.1:${port}`;
  const dev = given ? null : spawn(process.execPath, [wrangler, 'dev', '--ip', '127.0.0.1', '--port', String(port), '--inspector-port', String(await freePort()), '--local', '--show-interactive-dev-session=false'], { cwd: cloud, stdio: 'ignore' });
  let app = null, browser = null;
  try {
    for (let t0 = Date.now(); dev; ) { try { if ((await fetch(base + '/health')).ok) break; } catch { /* 아직 */ } if (Date.now() - t0 > 90000) throw new Error('wrangler dev 가 뜨지 않음'); await sleep(300); }
    log(`클라우드 ${base}`);
    const login = async role => {
      const r = await fetch(base + '/auth/login', { method: 'POST', headers: { 'content-type': 'application/json', 'x-worshipcue': '1', ...ipH(`198.19.0.${Math.floor(Math.random() * 200) + 1}`) }, body: JSON.stringify({ church: S.church, secret: C(role).secret }) });
      if (!r.ok) throw new Error(`로그인 ${role} ${r.status}`);
      return r.headers.getSetCookie().map(c => c.split(';')[0]).join('; ');
    };
    const owner = await login('owner');
    const api = async (path, body, cookie = owner) => {
      const r = await fetch(base + path, body ? { method: 'POST', headers: { cookie, origin: base, 'content-type': 'application/json', 'x-worshipcue': '1' }, body: JSON.stringify(body) } : { headers: { cookie } });
      return { status: r.status, json: await r.json().catch(() => null) };
    };

    // 2) 격리한 자료 폴더 + 설치판
    const data = join(out, 'data');
    const legacy = join(out, 'empty-legacy');
    mkdirSync(data, { recursive: true });
    mkdirSync(legacy, { recursive: true });
    writeFileSync(join(data, 'bible.db'), ''); // 빈 DB 를 먼저 둬서 실제 1.x 자료를 복사하지 않게
    const localPort = await freePort();
    writeFileSync(join(data, 'settings.json'), JSON.stringify({
      port: localPort, uiLang: 'ko', autoUpdate: false, telemetry: false, globalShortcuts: false,
      remote: { enabled: false }, lanOutput: { enabled: false }, webEditor: { enabled: false }, output: { autoOnSend: false }, syncDir: '',
      cloud: { enabled: true, url: base, key: C('bridge').key, remote: true, editor: true }, // 평문 key 는 점검용 (화면에서 넣으면 암호화)
    }, null, 2));
    const env = { ...process.env, WP_LEGACY_DATA: legacy, BIBLECAST_DATA: legacy };
    const launch = (extra = []) => spawn(exe, ['--data', data, '--win', '1600x960', ...extra], { cwd: repo, env, stdio: 'ignore' });
    const room = async () => (await api('/api/v1/room')).json || {};
    const waitOnline = async (want = true, ms = 30000) => { for (let t0 = Date.now(); Date.now() - t0 < ms; await sleep(300)) if (!!(await room()).online === want) return true; return false; };

    app = launch();
    check('설치판이 켜지면 클라우드에 붙는다', await waitOnline(true), `app=${(await room()).app || ''}`);

    // 보는 쪽 (node WebSocket): 출력 열쇠로
    const outRes = await fetch(`${base}/outputs/main?key=${encodeURIComponent(C('output').key)}`, { redirect: 'manual', headers: ipH('198.19.1.9') });
    const outCookie = outRes.headers.getSetCookie().map(c => c.split(';')[0]).join('; ');
    const viewer = new WebSocket(base.replace('http', 'ws') + '/events?client=main', { headers: { cookie: outCookie, origin: base } });
    const got = [];
    viewer.on('message', d => { try { got.push(JSON.parse(d.toString())); } catch { /* pong */ } });
    await new Promise((res, rej) => { viewer.once('open', res); viewer.once('error', rej); });
    const waitMsg = async (pred, ms = 15000) => { for (let t0 = Date.now(); Date.now() - t0 < ms; await sleep(100)) { const i = got.findIndex(pred); if (i >= 0) return got.splice(i, 1)[0]; } return null; };
    check('클라우드 출력이 붙자마자 설치판의 지금 화면을 받는다', await waitMsg(m => m.t === 'render' && m.data && m.data.output === 'main'));

    // 3) 웹 편집기로 견본 큐시트
    const made = await api('/e/new', { name: '견본 주일 예배' }, owner);
    check('클라우드 웹 편집기 → 설치판에 새 큐시트', made.status === 200 && made.json?.cue?.id, made.json?.cue?.id);
    const cueId = made.json.cue.id;
    const saved = await api('/e/save', { id: cueId, base: made.json.cue.updatedAt, name: '견본 주일 예배', items: [
      { type: 'title', title: '견본 교회 주일 예배', subtitle: '웹판 1단계 시험' },
      { type: 'song', title: '견본 찬양', lyrics: '[1절]\n견본 가사 첫째 줄\n클라우드로 보내는 둘째 줄\n\n[후렴]\n교회 밖에서도 보이는\n견본 후렴 가사' },
      { type: 'text', title: '광고', body: '이 글은 견본입니다\n---\n둘째 장 광고' },
    ] });
    check('클라우드 웹 편집기 저장 (순서 3개)', saved.status === 200 && saved.json?.cue?.items?.length === 3, `status ${saved.status}`);
    const stale = await api('/e/save', { id: cueId, base: made.json.cue.updatedAt, items: [] });
    check('예전 판으로 저장하면 409 충돌 (덮어쓰지 않음)', stale.status === 409 && stale.json?.conflict === true);

    // 설치판을 껐다 켬: 끊김 알림 → 다시 이음 → 화면을 이어 받음 (새 큐시트가 처음 큐시트라 열린다)
    kill(app);
    check('설치판을 끄면 클라우드 화면에 끊김이 간다', await waitMsg(m => m.t === 'status' && m.online === false, 20000));
    const r503 = await api('/r/do', { action: 'next' }, owner);
    check('설치판이 없을 때 리모컨 조작은 503 (쌓아 두지 않음)', r503.status === 503);
    await sleep(800);
    app = launch();
    check('설치판을 다시 켜면 다시 붙는다', await waitMsg(m => m.t === 'status' && m.online === true, 30000));
    await waitOnline(true);
    const st = await api('/e/state');
    check('다시 켠 설치판에 견본 큐시트가 열려 있다', st.json?.active === cueId, st.json?.active);

    // 4) 브라우저 (헤드리스 엣지): 클라우드 출력 셋 + 로컬 출력 하나
    browser = await cdp.launch(join(out, 'edge-profile'));
    const outUrl = p => `${base}/outputs/${p}?key=${encodeURIComponent(C('output').key)}`;
    const pages = {};
    for (const [name, url, w, h] of [['cloud-main', outUrl('main'), 1280, 720], ['cloud-overlay', outUrl('overlay'), 1280, 720], ['cloud-stage', outUrl('stage'), 1280, 720], ['local-main', `http://127.0.0.1:${localPort}/outputs/main`, 1280, 720]]) {
      pages[name] = await browser.open(url, w, h);
    }
    await sleep(2500);
    // 클라우드 리모컨(PIN 로그인)으로 송출
    const pinRes = await fetch(`${base}/r/state?pin=${C('remote').secret}&c=${S.church}`, { headers: ipH('198.19.2.7') });
    const remote = pinRes.headers.getSetCookie().map(c => c.split(';')[0]).join('; ');
    check('리모컨 PIN 로그인', pinRes.status === 200);
    const sendCue = await api('/r/do', { action: 'cue', index: 1 }, remote);
    check('클라우드 리모컨 → 찬양 순서 송출', sendCue.json?.ok === true, JSON.stringify(sendCue.json));
    const live1 = await waitMsg(m => m.t === 'render' && JSON.stringify(m.data?.slide || {}).includes('견본 가사 첫째 줄'));
    check('클라우드 출력이 송출을 따라온다 (1절)', live1, live1 ? `version ${live1.data.version}` : '');
    const seen = async (page, text) => page.waitFor(`document.body.innerText.includes(${JSON.stringify(text)})`, 15000);
    check('클라우드 예배당 화면 (브라우저)에 1절이 보인다', await seen(pages['cloud-main'], '견본 가사 첫째 줄'));
    check('로컬 예배당 화면에도 같은 1절', await seen(pages['local-main'], '견본 가사 첫째 줄'));
    for (const n of Object.keys(pages)) { await pages[n].settle(); await pages[n].shot(join(out, `1-${n}.png`)); }
    // 다음 (F6) → 후렴, 무대 메시지
    await api('/r/do', { action: 'next' }, remote);
    await api('/r/do', { action: 'message', text: '설교 5분 남음 (견본)' }, remote);
    check('클라우드 리모컨 → 다음 장 (후렴)', await waitMsg(m => m.t === 'render' && JSON.stringify(m.data?.slide || {}).includes('견본 후렴 가사')));
    check('클라우드 예배당 화면에 후렴', await seen(pages['cloud-main'], '견본 후렴 가사'));
    check('클라우드 무대 화면에 무대 메시지', await seen(pages['cloud-stage'], '설교 5분 남음'));
    for (const n of Object.keys(pages)) { await pages[n].settle(); await pages[n].shot(join(out, `2-${n}.png`)); }
    // 리모컨 페이지 · 주인 화면도 찍어 둔다 (클라우드 주소에서 열린 모습)
    const rp = await browser.open(`${base}/remote?c=${S.church}`, 420, 860);
    await rp.eval(`(async () => { const r = await fetch('/r/state?pin=${C('remote').secret}&c=${S.church}'); return r.status; })()`);
    await rp.eval('location.reload(), 1');
    await sleep(1500);
    await rp.eval(`(() => { const i = document.querySelector('input[type=password],input[inputmode=numeric],#pin'); if (i && !i.closest('[hidden]')) { i.value = '${C('remote').secret}'; const b = document.querySelector('#go,button[type=submit]'); b && b.click(); } return 1; })()`);
    check('클라우드 리모컨 페이지에 지금 송출 제목', await rp.waitFor(`document.body.innerText.includes('견본 찬양')`, 15000));
    await rp.settle(); await rp.shot(join(out, '3-cloud-remote.png'));
    const op = await browser.open(`${base}/login?next=/owner`, 900, 760);
    await op.eval(`(() => { document.getElementById('church').value = ${JSON.stringify(S.church)}; document.getElementById('secret').value = ${JSON.stringify(C('owner').secret)}; document.getElementById('f').requestSubmit(); return 1; })()`);
    check('주인 화면: 설치판 연결됨 · 붙은 화면 수', await op.waitFor(`document.body.innerText.includes('설치판 연결됨')`, 15000));
    await op.settle(); await op.shot(join(out, '4-cloud-owner.png'));
    // 로그인 없이 열면 로그인 화면으로
    const anonBrowser = await cdp.launch(join(out, 'edge-anon'));
    const anon = await anonBrowser.open(`${base}/outputs/main`, 900, 600);
    check('로그인 없이 연 출력 주소는 로그인 화면', await anon.waitFor(`location.pathname === '/login'`, 10000));
    await anon.shot(join(out, '5-anon-login.png'));
    try { anonBrowser.close(); } catch { /* 무시 */ }
    viewer.close();

    // 설치판 클라우드 연결 창 (설치판 화면 그림)
    kill(app); await sleep(800);
    const shot = join(out, '6-app-cloud-window.png');
    app = spawn(exe, ['--data', data, '--win', '1600x960', '--tool', 'cloud', '--shot', shot], { cwd: repo, env, stdio: 'ignore' });
    await new Promise(r => { const t = setTimeout(r, 60000); app.once('exit', () => { clearTimeout(t); r(); }); });
    check('설치판 클라우드 연결 창 그림', existsSync(shot));
  } finally {
    if (browser) { try { await browser.close(); } catch { /* 무시 */ } }
    kill(app);
    kill(dev);
  }
  writeFileSync(join(out, 'result.json'), JSON.stringify(results, null, 2));
  const bad = results.filter(r => !r.ok);
  console.log(`\n${results.length - bad.length}/${results.length} 통과 → ${out}`);
  process.exit(bad.length ? 1 : 0);
}
main().catch(e => { console.error(e); process.exit(1); });
