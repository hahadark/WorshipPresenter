// 뷰어 하나만 보기 비교 캡처 (격리 실행, 점검 자료로만)
//   node tools/viewers/shots.js --exe target\debug\wp-app.exe --seed tmp\vv\seed --cue tmp\vv\cue-long.json --out tmp\vv\shots [--only id]
// 경우마다 seed 를 새로 복사하고(WP_LEGACY_DATA = 빈 폴더), 앱이 --shot 으로 찍고 스스로 꺼진다. 자기가 띄운 PID 만 끈다.
const fs = require('fs');
const path = require('path');
const { spawn } = require('child_process');

const arg = (n, d) => { const i = process.argv.indexOf(n); return i > 0 ? process.argv[i + 1] : d; };
const exe = path.resolve(arg('--exe', 'target/debug/wp-app.exe'));
const seed = path.resolve(arg('--seed', 'tmp/vv/seed'));
const cue = path.resolve(arg('--cue', 'tmp/vv/cue-long.json'));
const out = path.resolve(arg('--out', 'tmp/vv/shots'));
const only = (arg('--only', '') || '').split(',').filter(Boolean);
const jobs = Number(arg('--jobs', '3'));
const work = path.join(path.dirname(out), 'run-' + path.basename(out));
fs.mkdirSync(out, { recursive: true });
fs.mkdirSync(work, { recursive: true });

const S = [];
const add = (id, args, set = {}) => S.push({ id, args, set });
const base = ['--import-cue', cue, '--item', '0', '--send'];
for (const w of ['1000x640', '1600x960', '1920x1080']) {
  add(`both-${w}`, ['--win', w, ...base]);
  add(`side-${w}`, ['--win', w, ...base], { 'rsProLayout.viewerMode': 'live' });

  add(`direct-${w}`, ['--win', w, ...base], { previewMode: false });
}
add('light-side-1600x960', ['--win', '1600x960', ...base], { 'rsProLayout.viewerMode': 'live', 'ui.theme': 'pro-light' });
add('en-side-1600x960', ['--win', '1600x960', ...base], { 'rsProLayout.viewerMode': 'live', uiLang: 'en' });
add('menu-view-1600x960', ['--win', '1600x960', '--import-cue', cue, '--open-menu', '보기']);

const list = only.length ? S.filter(s => only.some(o => s.id === o || s.id.startsWith(o))) : S;
function setPath(o, k, v) { const p = k.split('.'); let x = o; for (const a of p.slice(0, -1)) { if (typeof x[a] !== 'object' || !x[a]) x[a] = {}; x = x[a]; } x[p[p.length - 1]] = v; }
function copyDir(a, b) { fs.mkdirSync(b, { recursive: true }); for (const f of fs.readdirSync(a)) { const s = path.join(a, f), d = path.join(b, f); if (fs.statSync(s).isDirectory()) copyDir(s, d); else fs.copyFileSync(s, d); } }

function run(s) {
  return new Promise(res => {
    const d = path.join(work, s.id + '-' + Date.now().toString(36));
    copyDir(seed, d);
    const legacy = d + '-legacy'; fs.mkdirSync(legacy, { recursive: true });
    const sp = path.join(d, 'settings.json');
    const st = JSON.parse(fs.readFileSync(sp, 'utf8'));
    st.autoUpdate = false; st.telemetry = false; st.syncDir = ''; st.globalShortcuts = false; setPath(st, 'output.autoOnSend', false);
    for (const [k, v] of Object.entries(s.set)) setPath(st, k, v);
    fs.writeFileSync(sp, JSON.stringify(st, null, 1));
    const file = path.join(out, s.id + '.png');
    const p = spawn(exe, ['--data', d, ...s.args, '--shot', file], { env: { ...process.env, COMPUTERNAME: 'TEST-PC', WP_LEGACY_DATA: legacy }, stdio: 'ignore' });
    const t = setTimeout(() => { try { p.kill(); } catch {} }, 45000);
    p.on('exit', code => { clearTimeout(t); console.log(fs.existsSync(file) ? 'ok  ' : 'MISS', s.id, code); res(); });
  });
}

(async () => {
  const q = [...list];
  await Promise.all(Array.from({ length: jobs }, async () => { while (q.length) await run(q.shift()); }));
})();
