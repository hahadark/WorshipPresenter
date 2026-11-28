// 주 화면 다듬기(craft-main)용 캡처 — 같은 자료 · 같은 상태 · 같은 크기로 전후 쌍을 찍는다 (격리 실행)
//   node tools/craft/shots.js --exe target\debug\wp-app.exe --seed tmp\craft\seed --out tmp\craft\after [--only id,id] [--jobs 3]
// 경우마다 seed 를 새로 복사하고(WP_LEGACY_DATA = 빈 폴더, COMPUTERNAME = TEST-PC), 앱이 --shot 으로 찍고 스스로 꺼진다.
// 자기가 띄운 PID 만 끈다. seed 는 가상 큐시트(○○교회) · 개역한글만 든 시험 자료(tools/polish/shots.js 와 같은 것).
// 송출 템플릿은 로고 없는 'reading' 으로 바꿔 찍는다 (캡처에 교단 로고가 찍히지 않게).
const fs = require('fs');
const path = require('path');
const { spawn } = require('child_process');

const arg = (n, d) => { const i = process.argv.indexOf(n); return i > 0 ? process.argv[i + 1] : d; };
const exe = path.resolve(arg('--exe', 'target/debug/wp-app.exe'));
const seed = path.resolve(arg('--seed', 'tmp/craft/seed'));
const cue = path.resolve(arg('--cue', path.join(__dirname, 'cue.json')));
const out = path.resolve(arg('--out', 'tmp/craft/after'));
const only = (arg('--only', '') || '').split(',').filter(Boolean);
const jobs = Number(arg('--jobs', '3'));
const work = path.join(path.dirname(out), 'run-' + path.basename(out));
fs.mkdirSync(out, { recursive: true });
fs.mkdirSync(work, { recursive: true });

const W = '1600x960';
const base = ['--import-cue', cue];
// [이름, 인자, 설정 바꿈, 환경]
const S = [];
const add = (id, args, set = {}, env = {}) => S.push({ id, args, set, env });
// ---- 주 화면: 크기 · 배율 · 테마 · 언어
const live = [...base, '--item', '5', '--send'];
add('main-1000', ['--win', '1000x640', ...live]);
add('main-1600', ['--win', W, ...live]);
add('main-1920', ['--win', '1920x1080', ...live]);
add('main-s125', ['--win', W, ...live], { 'ui.scale': 125 });
add('main-s150', ['--win', W, ...live], { 'ui.scale': 150 });
add('main-1000-s150', ['--win', '1000x640', ...live], { 'ui.scale': 150 });
add('main-1920-s200', ['--win', '1920x1080', ...live], { 'ui.scale': 200 });
add('main-1000-en', ['--win', '1000x640', ...live], { uiLang: 'en' });
add('main-en', ['--win', W, ...live], { uiLang: 'en' });
add('main-prolight', ['--win', W, ...live], { 'ui.theme': 'pro-light' });
add('main-dark', ['--win', W, ...live], { 'ui.theme': 'dark' });
add('main-light', ['--win', W, ...live], { 'ui.theme': 'light' });
add('main-contrast', ['--win', W, ...live], { 'ui.theme': 'contrast' });
// ---- 상태: 빈 큐시트 · 고르기만 · 성경 찾기 · 바로 송출 · 송출 중만 · 꺼짐 알림 · 편집칸
add('main-empty', ['--win', W], { activeCueId: '' });
add('main-preview', ['--win', W, ...base, '--item', '1']);
add('main-bible', ['--win', W, ...base, '--search', '요3:16']);
add('search-bad', ['--win', W, ...base, '--search', '요22:1']);
add('main-bible-en', ['--win', W, ...base, '--search', '요3:16'], { uiLang: 'en' });
add('main-direct', ['--win', W, ...live], { previewMode: false });
add('main-liveonly', ['--win', W, ...live], { 'rsProLayout.viewerMode': 'live' });
add('main-notify', ['--win', W, ...base, '--item', '2', '--send'], { 'output.autoOnSend': false });
add('main-left-bible', ['--win', W, ...base, '--left-tab', 'books']);
add('main-left-lib', ['--win', W, ...base, '--left-tab', 'library']);
add('main-edit', ['--win', W, '--edit-actions']);
for (const t of ['text', 'stage', 'media', 'sources']) add('ins-' + t, ['--win', W, ...base, '--item', '4', '--ins-tab', t]);
// ---- 메뉴 (모양 · 간격만 — 항목 구성은 손대지 않음)
for (const m of ['파일', '보기']) add('menu-' + m, ['--win', W, ...base, '--open-menu', m]);
// ---- 견본 창 (디자인 체계: 모든 부품 · 상태 · 글자 단계 · 색)
for (const th of ['pro', 'pro-light', 'dark', 'contrast']) add('specimen-' + th, ['--win', '1600x1500', '--specimen'], { 'ui.theme': th });
add('specimen-s125', ['--win', '1600x1500', '--specimen'], { 'ui.scale': 125 });
add('specimen-s150', ['--win', '1600x1500', '--specimen'], { 'ui.scale': 150 });
add('icons', ['--win', '1200x900', '--specimen', 'icons']);
add('specimen-windows', ['--win', '1200x800', '--specimen', 'windows']);
add('specimen-windows-light', ['--win', '1200x800', '--specimen', 'windows'], { 'ui.theme': 'pro-light' });
add('icons-s125', ['--win', '1200x900', '--specimen', 'icons'], { 'ui.scale': 125 });
add('icons-s150', ['--win', '1200x900', '--specimen', 'icons'], { 'ui.scale': 150 });
add('icons-old', ['--win', '1200x900', '--specimen', 'icons'], {}, { WP_ICONS: 'lucide' });

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
    // 격리: 업데이트 확인 · 통계 · 동기화 · 리모컨 · 내부망 · 전역 단축키 · 예배당 창 자동 켜기 끔
    st.autoUpdate = false; st.telemetry = false; st.syncDir = ''; st.globalShortcuts = false;
    setPath(st, 'output.autoOnSend', false); setPath(st, 'remote.enabled', false); setPath(st, 'lanOutput.enabled', false);
    setPath(st, 'activeTemplates.main', 'reading'); // 로고 없는 템플릿
    for (const [k, v] of Object.entries(s.set)) setPath(st, k, v);
    fs.writeFileSync(sp, JSON.stringify(st, null, 1));
    const file = path.join(out, s.id + '.png');
    try { fs.unlinkSync(file); } catch {}
    const p = spawn(exe, ['--data', d, ...s.args, '--shot', file], { env: { ...process.env, COMPUTERNAME: 'TEST-PC', WP_LEGACY_DATA: legacy, ...s.env }, stdio: 'ignore' });
    const t = setTimeout(() => { try { p.kill(); } catch {} }, 45000);
    p.on('exit', code => { clearTimeout(t); console.log(fs.existsSync(file) ? 'ok  ' : 'MISS', s.id, code); res(); });
  });
}

(async () => {
  const q = [...list];
  await Promise.all(Array.from({ length: jobs }, async () => { while (q.length) await run(q.shift()); }));
})();
