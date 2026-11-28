// 창 · 대화상자 다듬기(craft-windows)용 캡처 — 점검 자료로만, 격리 실행
//   node tools/craft/shots.js --exe target\debug\wp-app.exe --seed tmp\craft\seed --cue tmp\craft\cue.json --out tmp\craft\before [--only id,id] [--jobs 3] [--sets base,sizes,en,states]
// 경우마다 seed 를 새로 복사하고(WP_LEGACY_DATA = 빈 폴더), 앱이 --shot 으로 찍고 스스로 꺼진다. 자기가 띄운 PID 만 끈다.
// 주 템플릿은 로고 없는 것으로 바꿔 찍는다 (캡처에 교단 로고가 나오지 않게). COMPUTERNAME=TEST-PC.
const fs = require('fs');
const path = require('path');
const { spawn } = require('child_process');

const arg = (n, d) => { const i = process.argv.indexOf(n); return i > 0 ? process.argv[i + 1] : d; };
const exe = path.resolve(arg('--exe', 'target/debug/wp-app.exe'));
const seed = path.resolve(arg('--seed', 'tmp/craft/seed'));
const cue = path.resolve(arg('--cue', 'tmp/craft/cue.json'));
const out = path.resolve(arg('--out', 'tmp/craft/before'));
const only = (arg('--only', '') || '').split(',').filter(Boolean);
const sets = (arg('--sets', 'base') || '').split(',').filter(Boolean);
const jobs = Number(arg('--jobs', '3'));
const work = path.join(path.dirname(out), 'run-' + path.basename(out));
fs.mkdirSync(out, { recursive: true });
fs.mkdirSync(work, { recursive: true });

const W = '1600x960';
const base = ['--import-cue', cue];
const S = [];
const add = (set, id, args, st = {}, env = {}) => S.push({ set, id, args, st, env });

// ---- 창 목록 (docs/ui-craft-windows.md 의 표와 같은 이름)
// [이름, 여는 인자]
const WINS = [
  // 파일 · 설정
  ['settings', ['--open-menu', 'c:settings']],
  ['set-send', ['--settings-cat', 'send']], ['set-output', ['--settings-cat', 'output']], ['set-obs', ['--settings-cat', 'obs']], ['set-ui', ['--settings-cat', 'ui']],
  ['set-keys', ['--settings-cat', 'keys']], ['set-data', ['--settings-cat', 'data']], ['set-backup', ['--settings-cat', 'backup']], ['set-remote', ['--settings-cat', 'remote']], ['set-about', ['--settings-cat', 'about']],
  ['set-search', ['--settings-q', '글꼴']], ['set-search-none', ['--settings-q', '없는말없는말']],
  ['templates', ['--open-menu', 'c:templates']], ['tpl', ['--tpl-open', 'main']], ['tpl-overlay', ['--tpl-open', 'overlay']],
  ['themes', ['--open-menu', 'c:themes']],
  ['cues', ['--item', '1', '--open-menu', 'b:cues']],
  ['slideedit', ['--item', '1', '--open-menu', 'c:slideedit']],
  ['songs', ['--open-menu', 'b:songs']], ['lib', ['--open-menu', 'b:lib']],
  ['import-bible', ['--open-menu', 'c:import-bible']], ['commentary', ['--open-menu', 'c:commentary']],
  ['history', ['--item', '1', '--send', '--open-menu', 'c:history']], ['usage', ['--open-menu', 'c:usage']], ['about', ['--open-menu', 'c:about']],
  ['sync', ['--open-menu', 'b:sync']], ['diagnose', ['--open-menu', 'b:diagnose']], ['print', ['--open-menu', 'b:print']],
  // 보기 · 송출
  ['scene', ['--scene-open', 'pip']], ['scene-empty', ['--scene-open']], ['output', ['--open-menu', 'd:output']],
  ['workspaces', ['--open-menu', 'w:workspaces']],
  ['overlays', ['--open-menu', 'b:overlays']], ['message', ['--open-menu', 'b:message']], ['names', ['--open-menu', 'b:names']], ['quick', ['--open-menu', 'b:quick']],
  ['stage', ['--open-menu', 'b:stage']], ['macros', ['--open-menu', 'b:macros']], ['draw', ['--item', '1', '--send', '--open-menu', 'b:draw']],
  // 도구
  ['cameras', ['--open-menu', 'b:cameras']], ['camtest', ['--open-menu', 'd:camtest']], ['obs', ['--open-menu', 'd:obs']], ['bulletin', ['--open-menu', 'd:bulletin']],
  ['update', ['--open-menu', 'd:update']],
  ['tool-remote', ['--tool', 'remote']], ['tool-trigger', ['--tool', 'trigger']], ['tool-devices', ['--tool', 'devices']], ['tool-lan', ['--tool', 'lan']], ['tool-phonecam', ['--tool', 'phonecam']], ['tool-webedit', ['--tool', 'webedit']],
  ['home', ['--open-menu', 'a:home']], ['points', ['--open-menu', 'a:points']], ['preservice', ['--open-menu', 'a:preservice']],
  ['help-keys', ['--open-menu', 'a:help-keys']],
  ['paste', ['--open-a2', 'paste']],
  // AI
  ['ai-settings', ['--open-menu', 'ai:settings']], ['ai-check', ['--open-menu', 'ai:check']], ['ai-sermon', ['--open-menu', 'ai:sermon']], ['ai-listen', ['--open-menu', 'ai:listen']],
  ['ai-lyrics', ['--open-menu', 'ai:lyrics']], ['ai-recap', ['--open-menu', 'ai:recap']], ['ai-say', ['--open-menu', 'ai:say']],
  // 점검 인자 --open-x 로만 열리는 것 (craft-windows 에서 더함)
  // (찬송가 · PPT 넣기 · 큰 PPT 나누기 · 첫 실행 자료 옮김 · 인터넷 자료 안내는 점검 인자가 없어 코드로만 봄 — docs/ui-craft-windows.md)
  ['x-lyrics', ['--open', 'lyrics']], ['x-aiconsent', ['--open-x', 'aiconsent']],
  ['x-update-new', ['--open-x', 'update-new']], ['x-update-err', ['--open-x', 'update-err']], ['x-update-dl', ['--open-x', 'update-dl']],
];
for (const [id, a] of WINS) add('base', id, ['--win', W, ...base, ...a]);
// 주 화면 기준 한 장 (창이 얹히는 바탕)
add('base', 'main', ['--win', W, ...base, '--item', '1', '--send']);

// ---- 크기 · 배율: 창이 넘치거나 잘리는지
const SIZED = ['settings', 'set-keys', 'tpl', 'themes', 'cues', 'slideedit', 'scene', 'output', 'update', 'songs', 'lib', 'ai-listen', 'ai-lyrics', 'ai-recap', 'sync', 'diagnose', 'stage', 'message', 'macros', 'names', 'import-bible', 'points', 'tool-devices', 'tool-remote', 'help-keys', 'x-update-new', 'bulletin', 'obs', 'usage', 'draw', 'ai-sermon', 'ai-check', 'ai-settings', 'preservice', 'home', 'camtest', 'cameras', 'overlays'];
for (const id of SIZED) {
  const a = WINS.find(w => w[0] === id)[1];
  add('sizes', id + '@1000', ['--win', '1000x640', ...base, ...a]);
  add('sizes', id + '@150', ['--win', W, ...base, ...a], { 'ui.scale': 150 });
  add('sizes', id + '@1000s150', ['--win', '1000x640', ...base, ...a], { 'ui.scale': 150 });
}
for (const id of ['settings', 'tpl', 'cues', 'scene', 'slideedit', 'output', 'update', 'themes']) {
  const a = WINS.find(w => w[0] === id)[1];
  add('sizes', id + '@1920', ['--win', '1920x1080', ...base, ...a]);
  add('sizes', id + '@125', ['--win', W, ...base, ...a], { 'ui.scale': 125 });
}
// ---- 영어
for (const [id, a] of WINS) add('en', 'en-' + id, ['--win', W, ...base, ...a], { uiLang: 'en' });
// ---- 테마: 전문가 밝게 · 밝게 · 고대비 몇 개
for (const th of ['pro-light', 'light', 'contrast']) for (const id of ['settings', 'tpl', 'cues', 'scene', 'output', 'update', 'slideedit', 'x-update-new', 'message']) {
  const a = WINS.find(w => w[0] === id)[1];
  add('themes', `${th}-${id}`, ['--win', W, ...base, ...a], { 'ui.theme': th });
}
// ---- 빈 상태 (큐시트 · 자료 없음)
for (const id of ['cues', 'songs', 'lib', 'history', 'usage', 'overlays', 'macros', 'names', 'scene-empty', 'slideedit', 'themes', 'commentary', 'ai-check', 'stage']) {
  const a = WINS.find(w => w[0] === id)[1];
  add('states', 'empty-' + id, ['--win', W, ...a.filter(x => x !== '--send')], { activeCueId: '', scenes: [], macros: [], overlays: [], people: [] }, { WP_CRAFT_EMPTY: '1' });
}

const list = S.filter(s => sets.includes(s.set) || sets.includes('all')).filter(s => !only.length || only.some(o => s.id === o || s.id.startsWith(o)));

function setPath(o, k, v) { const p = k.split('.'); let x = o; for (const a of p.slice(0, -1)) { if (typeof x[a] !== 'object' || !x[a]) x[a] = {}; x = x[a]; } x[p[p.length - 1]] = v; }
function copyDir(a, b) { fs.mkdirSync(b, { recursive: true }); for (const f of fs.readdirSync(a)) { const s = path.join(a, f), d = path.join(b, f); if (fs.statSync(s).isDirectory()) copyDir(s, d); else fs.copyFileSync(s, d); } }

function run(s) {
  return new Promise(res => {
    const d = path.join(work, s.id.replace(/[^\w.-]/g, '_') + '-' + Date.now().toString(36));
    copyDir(seed, d);
    const legacy = d + '-legacy'; fs.mkdirSync(legacy, { recursive: true });
    const sp = path.join(d, 'settings.json');
    const st = JSON.parse(fs.readFileSync(sp, 'utf8'));
    st.autoUpdate = false; st.telemetry = false; st.syncDir = ''; setPath(st, 'output.autoOnSend', false);
    st.globalShortcuts = false; if (st.remote) st.remote.enabled = false; if (st.lanOutput && typeof st.lanOutput === 'object') st.lanOutput.enabled = false;
    setPath(st, 'activeTemplates.main', 'theme-bc-clean-lyrics-main'); // 로고 없는 템플릿
    for (const [k, v] of Object.entries(s.st)) setPath(st, k, v);
    fs.writeFileSync(sp, JSON.stringify(st, null, 1));
    const file = path.join(out, s.id + '.png');
    try { fs.unlinkSync(file); } catch {}
    const p = spawn(exe, ['--data', d, ...s.args, '--shot', file], { env: { ...process.env, COMPUTERNAME: 'TEST-PC', WP_LEGACY_DATA: legacy, ...s.env }, stdio: 'ignore' });
    const t = setTimeout(() => { try { p.kill(); } catch {} }, 60000);
    p.on('exit', code => {
      clearTimeout(t);
      console.log(fs.existsSync(file) ? 'ok  ' : 'MISS', s.id, code);
      try { fs.rmSync(d, { recursive: true, force: true }); fs.rmSync(legacy, { recursive: true, force: true }); } catch {}
      res();
    });
  });
}

(async () => {
  console.log(list.length + '장');
  const q = [...list];
  await Promise.all(Array.from({ length: jobs }, async () => { while (q.length) await run(q.shift()); }));
})();
