// 인터페이스 다듬기용 전체 캡처 (점검 자료로만, 격리 실행)
//   node tools/polish/shots.js --exe target\debug\wp-app.exe --seed tmp\polish\seed --cue tmp\polish\cue.json --out tmp\polish\before [--only id,id] [--jobs 3]
// 경우마다 seed 를 새로 복사하고(WP_LEGACY_DATA = 빈 폴더), 앱이 --shot 으로 찍고 스스로 꺼진다. 자기가 띄운 PID 만 끈다.
const fs = require('fs');
const path = require('path');
const { spawn } = require('child_process');

const arg = (n, d) => { const i = process.argv.indexOf(n); return i > 0 ? process.argv[i + 1] : d; };
const exe = path.resolve(arg('--exe', 'target/debug/wp-app.exe'));
const seed = path.resolve(arg('--seed', 'tmp/polish/seed'));
const cue = path.resolve(arg('--cue', 'tmp/polish/cue.json'));
const out = path.resolve(arg('--out', 'tmp/polish/before'));
const only = (arg('--only', '') || '').split(',').filter(Boolean);
const jobs = Number(arg('--jobs', '3'));
const work = path.join(path.dirname(out), 'run-' + path.basename(out));
fs.mkdirSync(out, { recursive: true });
fs.mkdirSync(work, { recursive: true });

const W = '1600x960';
const base = ['--import-cue', cue];
// [이름, 인자, 설정 바꿈]
const S = [];
const add = (id, args, set = {}, env = {}) => S.push({ id, args, set, env });
// 주 화면 · 크기 · 배율
add('main-1000', ['--win', '1000x640', ...base, '--item', '1', '--send']);
add('main-1600', ['--win', W, ...base, '--item', '1', '--send']);
add('main-1920', ['--win', '1920x1080', ...base, '--item', '1', '--send']);
add('main-s125', ['--win', W, ...base, '--item', '1', '--send'], { 'ui.scale': 125 });
add('main-s150', ['--win', W, ...base, '--item', '1', '--send'], { 'ui.scale': 150 });
add('main-light', ['--win', W, ...base, '--item', '1', '--send'], { 'ui.theme': 'light' });
add('main-contrast', ['--win', W, ...base, '--item', '1', '--send'], { 'ui.theme': 'contrast' });
add('main-en', ['--win', W, ...base, '--item', '1', '--send'], { uiLang: 'en' });
add('main-empty', ['--win', W], { activeCueId: '' });
add('main-bible', ['--win', W, '--search', '요3:16']);
add('main-left-bible', ['--win', W, ...base, '--left-tab', 'books']);
add('main-left-lib', ['--win', W, ...base, '--left-tab', 'library']);
add('main-notify', ['--win', W, ...base, '--item', '2', '--send'], { 'output.autoOnSend': false });
// 인스펙터 탭
for (const t of ['text', 'stage', 'media', 'sources']) add('ins-' + t, ['--win', W, ...base, '--item', '0', '--ins-tab', t]);
add('edit-actions', ['--win', W, '--edit-actions']);
// 메뉴
for (const m of ['파일', '편집', '보기', '큐시트', '화면', '연결', '송출', 'AI', '도움말']) add('menu-' + m, ['--win', W, ...base, '--open-menu', m]);
// 창 (메뉴 id)
const wins = ['c:settings', 'c:templates', 'c:history', 'c:usage', 'c:about', 'c:slideedit', 'c:themes', 'c:import-bible', 'c:commentary',
  'b:songs', 'b:lib', 'b:overlays', 'b:message', 'b:names', 'b:quick', 'b:stage', 'b:macros', 'b:cues', 'b:sync', 'b:diagnose', 'b:draw', 'b:cameras',
  'd:bulletin', 'd:output', 'd:obs', 'd:camtest', 'd:update',
  'a:home', 'a:points', 'a:preservice', 'a:help-keys',
  'ai:settings', 'ai:check', 'ai:sermon', 'ai:listen', 'ai:lyrics', 'ai:recap', 'ai:say', 'w:workspaces'];
for (const w of wins) add('win-' + w.replace(':', '-'), ['--win', W, ...base, '--item', '1', '--open-menu', w]);
add('win-scene', ['--win', W, ...base, '--scene-open', 'pip']);
add('win-tpl', ['--win', W, ...base, '--tpl-open', 'main']);
for (const c of ['send', 'output', 'obs', 'ui', 'keys', 'data', 'backup', 'remote', 'about']) add('set-' + c, ['--win', W, ...base, '--settings-cat', c]);
for (const t of ['remote', 'trigger', 'devices', 'lan', 'phonecam', 'webedit']) add('tool-' + t, ['--win', W, ...base, '--item', '1', '--tool', t]);
add('win-paste', ['--win', W, ...base, '--open-a2', 'paste']);
// 영어 화면 몇 개
for (const w of ['c:settings', 'd:output', 'ai:settings', 'b:cues', 'c:templates']) add('en-' + w.replace(':', '-'), ['--win', W, ...base, '--item', '1', '--open-menu', w], { uiLang: 'en' });
add('en-scene', ['--win', W, ...base, '--scene-open', 'pip'], { uiLang: 'en' });
add('en-ins-sources', ['--win', W, ...base, '--item', '0', '--ins-tab', 'sources'], { uiLang: 'en' });
// 밝게 · 고대비 창 몇 개
for (const th of ['light', 'contrast']) for (const w of ['c:settings', 'd:output', 'ai:check']) add(`${th}-${w.replace(':', '-')}`, ['--win', W, ...base, '--item', '1', '--open-menu', w], { 'ui.theme': th });
// 전문가 밝게 (2.1.0 다듬기 2차)
add('main-prolight', ['--win', W, ...base, '--item', '1', '--send'], { 'ui.theme': 'pro-light' });
add('main-prolight-1000', ['--win', '1000x640', ...base, '--item', '1', '--send'], { 'ui.theme': 'pro-light' });
for (const w of ['c:settings', 'd:output', 'b:cues', 'c:templates']) add(`prolight-${w.replace(':', '-')}`, ['--win', W, ...base, '--item', '1', '--open-menu', w], { 'ui.theme': 'pro-light' });
add('prolight-tpl', ['--win', W, ...base, '--tpl-open', 'main'], { 'ui.theme': 'pro-light' });
add('prolight-menu-연결', ['--win', W, ...base, '--open-menu', '연결'], { 'ui.theme': 'pro-light' });
// 좁은 창에서 창
for (const w of ['c:settings', 'd:output']) add('narrow-' + w.replace(':', '-'), ['--win', '1000x640', ...base, '--open-menu', w]);

const list = only.length ? S.filter(s => only.some(o => s.id === o || s.id.startsWith(o))) : S;

function setPath(o, k, v) { const p = k.split('.'); let x = o; for (const a of p.slice(0, -1)) { if (typeof x[a] !== 'object' || !x[a]) x[a] = {}; x = x[a]; } x[p[p.length - 1]] = v; }
function copyDir(a, b) { fs.mkdirSync(b, { recursive: true }); for (const f of fs.readdirSync(a)) { const s = path.join(a, f), d = path.join(b, f); if (fs.statSync(s).isDirectory()) copyDir(s, d); else fs.copyFileSync(s, d); } }

function run(s) {
  return new Promise(res => {
    const d = path.join(work, s.id.replace(/[^\w.-]/g, '_') + '-' + require('crypto').createHash('md5').update(s.id).digest('hex').slice(0, 6) + '-' + Date.now().toString(36));
    copyDir(seed, d);
    const legacy = d + '-legacy'; fs.mkdirSync(legacy, { recursive: true });
    const sp = path.join(d, 'settings.json');
    const st = JSON.parse(fs.readFileSync(sp, 'utf8'));
    st.autoUpdate = false; st.telemetry = false; st.syncDir = ''; setPath(st, 'output.autoOnSend', false);
    for (const [k, v] of Object.entries(s.set)) setPath(st, k, v);
    fs.writeFileSync(sp, JSON.stringify(st, null, 1));
    const file = path.join(out, s.id + '.png');
    const p = spawn(exe, ['--data', d, ...s.args, '--shot', file], { env: { ...process.env, COMPUTERNAME: 'TEST-PC', WP_LEGACY_DATA: legacy, ...s.env }, stdio: 'ignore' });
    const t = setTimeout(() => { try { p.kill(); } catch {} }, 45000);
    p.on('exit', code => { clearTimeout(t); console.log(fs.existsSync(file) ? 'ok  ' : 'MISS', s.id, code); res(); });
  });
}

(async () => {
  const q = [...list];
  await Promise.all(Array.from({ length: jobs }, async () => { while (q.length) await run(q.shift()); }));
})();
