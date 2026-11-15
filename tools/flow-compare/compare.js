// 예배 흐름 출력 대조: Electron 판과 Rust 판에 같은 큐시트를 처음부터 끝까지 한 순서·한 슬라이드씩 송출하며
// 웹 출력 3종(/outputs/main · overlay · stage)을 1920×1080 으로 찍어 비교한다.
//
// 준비 (tools/flow-compare/README.md):
//   Electron: <설치본 exe> --data tmp\el-data  (로그에서 포트 확인)
//   Rust    : target\debug\examples\flow_driver.exe --data tmp\rs-data   (tmp\rs-data\flow-port.txt · flow-ctl.txt)
// 실행: node tools\flow-compare\compare.js --el 37528 --rs-data tmp\rs-data --out tmp\flow-out [--cue <id>] [--items 0,1,2] [--max-slides 40]
'use strict';
const fs = require('fs');
const path = require('path');
const { launch, sleep } = require('./cdp');
const png = require('./png');

const A = process.argv.slice(2);
const opt = (k, d) => { const i = A.indexOf('--' + k); return i >= 0 ? A[i + 1] : d; };
const EL = `http://127.0.0.1:${opt('el', (() => { try { return JSON.parse(fs.readFileSync('tmp/flow-ports.json', 'utf8')).el; } catch { return ''; } })())}`;
const RSD = opt('rs-data', 'tmp/rs-data');
const RS = `http://127.0.0.1:${fs.readFileSync(path.join(RSD, 'flow-port.txt'), 'utf8').trim()}`;
const RSCTL = fs.readFileSync(path.join(RSD, 'flow-ctl.txt'), 'utf8').trim();
const OUT = opt('out', 'tmp/flow-out');
const MAXS = Number(opt('max-slides', '60'));
const SKIP = new Set((opt('skip-types', 'hymn,camera,video,audio,slideshow')).split(',')); // 찬송가·PPT 는 다른 갈래 — 장 수만 비교
const OUTPUTS = (opt('outputs', 'main,overlay,stage')).split(',');
fs.mkdirSync(OUT, { recursive: true });

async function rs(cmd, args = {}) {
  const r = await (await fetch(RSCTL, { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ cmd, args }) })).json();
  if (!r.ok) throw new Error(`Rust ${cmd}: ${r.error}`);
  return r.result;
}

// 슬라이드 → 비교용 글 (종류·제목·본문·장절·줄 나눔 위치)
function slideKey(s) {
  if (!s) return '';
  const t = s.type || 'bible';
  const parts = [t];
  if (s.verses) parts.push(s.verses.map(v => `${v.chapter || ''}:${v.verse}${v.part ? '#' + v.part : ''}=${(v.text || '').replace(/\s+/g, ' ').trim()}`).join(' | '));
  for (const k of ['ref', 'title', 'subtitle', 'text', 'label', 'body']) if (s[k] != null && typeof s[k] !== 'object') parts.push(`${k}=${String(s[k]).replace(/\s+/g, ' ').trim()}`);
  if (Array.isArray(s.lines)) parts.push('lines=' + s.lines.map(l => (typeof l === 'string' ? l : JSON.stringify(l))).join('/'));
  if (s.parallel) parts.push('par=' + JSON.stringify(s.parallel).replace(/\s+/g, ' ').slice(0, 400));
  return parts.join(' ‖ ');
}

// 시계·타이머처럼 흐르는 숫자는 빼고 비교 (무대 화면)
const outText = p => p.eval(`(() => { const b = document.body; return b ? b.innerText.replace(/\\d{1,2}:\\d{2}(:\\d{2})?/g, '#').replace(/(오전|오후|AM|PM)/g, '').replace(/\\s+/g, ' ').trim() : ''; })()`);
// 이 슬라이드에서 화면에 보여야 할 글의 앞머리 (서식 표시·빈칸은 빼고). 글이 없는 슬라이드(빈 화면·그림)는 ''
function expectText(s) {
  if (!s || ['blank', 'image', 'score', 'video', 'camera'].includes(s.kind)) return '';
  const raw = ((s.parts || []).map(p => p.text || '').find(t => t.trim()) || s.title || '').split('\n').map(l => l.replace(/\{[^}]*\}/g, '').replace(/[*_~^#>\-]/g, '')).find(l => l.replace(/\s/g, '').length >= 2) || '';
  return raw.replace(/\s+/g, '').slice(0, 6);
}
// '내용이 실제로 보일 때까지' 기다린다: 글이 DOM 에 있고, 그 글 영역의 불투명도가 1 이 될 때까지 (페이드 중·글이 뜨기 전에 찍으면 둘 다 빈 화면이라 0% 로 잘못 판정된다)
async function waitShown(page, want, maxMs = 5000) {
  if (!want) return true;
  return page.waitFor(`(() => { const a = [...document.querySelectorAll('.bc-stage:not(.bc-measure .bc-stage) > .bc-area')].filter(e => parseFloat(getComputedStyle(e).opacity) > 0.99);
    return a.some(e => (e.innerText || '').replace(/\\s+/g, '').includes(${JSON.stringify(want)})); })()`, maxMs);
}
// 그림 전체가 한 가지 색인가 (아무것도 안 그려짐)
function uniform(img) { const d = img.data; for (let i = 4; i < d.length; i += 4 * 97) if (Math.abs(d[i] - d[0]) > 6 || Math.abs(d[i + 1] - d[1]) > 6 || Math.abs(d[i + 2] - d[2]) > 6) return false; return true; }


async function main() {
  const br = await launch(path.join(OUT, 'profile'));
  const report = { started: new Date().toISOString(), el: EL, rs: RS, cues: [] };
  try {
    const ctl = await br.open(`${EL}/control/`, 1600, 950);
    if (!(await ctl.waitFor('!!(window.BCCue && window.BC && window.BC.state)', 30000))) throw new Error('Electron 조작 화면이 안 떴습니다');
    const ecmd = (n, a = {}) => ctl.eval(`BC.cmd(${JSON.stringify(n)}, ${JSON.stringify(a)})`);
    const pages = {};
    for (const o of OUTPUTS) {
      pages['el-' + o] = await br.open(`${EL}/outputs/${o}`);
      pages['rs-' + o] = await br.open(`${RS}/outputs/${o}`);
    }
    await sleep(1500);
    const cuesArg = opt('cue', '');
    const cueIds = cuesArg ? cuesArg.split(',') : (await ctl.eval(`BC.cmd('listCues').then(r => (r.result || []).map(c => c.id))`));
    for (const cueId of cueIds) {
      await ctl.eval(`BCCue.openCue(${JSON.stringify(cueId)})`);
      const nItems = (await rs('openCue', { id: cueId })).items;
      const info = await ctl.eval(`BCCue.cueInfo()`);
      const crep = { id: cueId, items: [] };
      report.cues.push(crep);
      console.log(`\n== 큐시트 ${cueId} (Electron ${info.items.length} · Rust ${nItems}) ==`);
      const only = opt('items', '');
      const idxs = only ? only.split(',').map(Number) : info.items.map((_, i) => i);
      for (const i of idxs) {
        const it = info.items[i];
        const irep = { index: i, type: it.type, title: it.title || it.ref || '', diffs: [] };
        crep.items.push(irep);
        let es = [], rd = null;
        try { es = await ctl.eval(`BCCue.slidesFor(${i})`, 90000); } catch (e) { irep.elError = e.message; }
        try { rd = await rs('slidesFor', { index: i }); } catch (e) { irep.rsError = e.message; }
        const rsl = rd ? rd.slides : [];
        irep.count = [es.length, rsl.length];
        const ek = es.map(slideKey), rk = rsl.map(slideKey);
        for (let k = 0; k < Math.max(ek.length, rk.length); k++) if (ek[k] !== rk[k]) irep.diffs.push({ slide: k, el: ek[k] || null, rs: rk[k] || null });
        console.log(`[${i}] ${it.type} ${irep.title.slice(0, 30)}: 장 수 E${es.length}/R${rsl.length}${irep.diffs.length ? ' · 내용 차이 ' + irep.diffs.length : ''}${irep.elError ? ' · E오류 ' + irep.elError : ''}${irep.rsError ? ' · R오류 ' + irep.rsError : ''}`);
        if (SKIP.has(it.type) || !es.length || !rsl.length) continue;
        // 송출 → 슬라이드마다 출력 비교
        await ctl.eval(`BCCue.sendItem(${i})`, 90000);
        await rs('sendItem', { index: i });
        irep.shots = [];
        const n = Math.min(es.length, rsl.length, MAXS);
        for (let k = 0; k < n; k++) {
          if (k > 0) { await ecmd('nextSlide'); await rs('nextSlide'); }
          for (const o of OUTPUTS) { await pages['el-' + o].settle(); await pages['rs-' + o].settle(); }
          const srep = { slide: k };
          // 예배당 화면에 이 슬라이드의 글이 실제로 보일 때까지
          const want = expectText(rsl[k]);
          if (OUTPUTS.includes('main') && want) {
            const [se, sr] = [await waitShown(pages['el-main'], want), await waitShown(pages['rs-main'], want)];
            if (!se || !sr) { srep.notShown = { want, el: se, rs: sr }; irep.notShown = (irep.notShown || 0) + 1; report.notShown = (report.notShown || 0) + 1; }
          }
          for (const o of OUTPUTS) {
            const [ea, ra] = [await pages['el-' + o].shot(), await pages['rs-' + o].shot()];
            const [ei, ri] = [png.decode(ea), png.decode(ra)];
            const d = png.diff(ei, ri);
            const [et, rt] = [await outText(pages['el-' + o]), await outText(pages['rs-' + o])];
            srep[o] = { px: Math.round(d.ratio * 10000) / 100, box: d.box, textSame: et === rt };
            // 글이 있어야 하는데 두 판 모두 한 가지 색이면 '같음' 이 아니라 '판정 못 함'
            if (o === 'main' && want && uniform(ei) && uniform(ri)) { srep[o].blankBoth = true; report.blankBoth = (report.blankBoth || 0) + 1; }
            report.checked = (report.checked || 0) + 1; if (srep[o].px > 0.4) report.pxDiff = (report.pxDiff || 0) + 1; if (et !== rt) report.textDiff = (report.textDiff || 0) + 1;
            if (et !== rt) srep[o].text = { el: et.slice(0, 300), rs: rt.slice(0, 300) };
            if (d.ratio > 0.004 || et !== rt) {
              const f = path.join(OUT, `${cueId}-${i}-${k}-${o}.png`);
              fs.writeFileSync(f, png.encode(png.sideBySide(ei, ri)));
              srep[o].file = f;
            }
          }
          irep.shots.push(srep);
          const bad = OUTPUTS.filter(o => srep[o].px > 0.4 || !srep[o].textSame || srep[o].blankBoth);
          if (bad.length || srep.notShown) console.log(`   장 ${k + 1}: ` + bad.map(o => `${o} ${srep[o].px}%${srep[o].textSame ? '' : ' 글 다름'}${srep[o].blankBoth ? ' 둘 다 빈 화면(판정 못 함)' : ''}`).join(' · ') + (srep.notShown ? ` · 내용 안 보임(${srep.notShown.want}) E${srep.notShown.el ? '보임' : '안 보임'} R${srep.notShown.rs ? '보임' : '안 보임'}` : ''));
        }
      }
      fs.writeFileSync(path.join(OUT, 'report.json'), JSON.stringify(report, null, 1));
    }
    const cnt = report.cues.flatMap(c => c.items).filter(i => i.count && i.count[0] !== i.count[1]).length, cdiff = report.cues.flatMap(c => c.items).filter(i => i.diffs.length).length, all = report.cues.flatMap(c => c.items).length;
    console.log(`\n== 순서 ${all}개: 장 수 다름 ${cnt} · 내용 다름 ${cdiff} | 출력 비교 ${report.checked || 0}건: 그림 다름(>0.4%) ${report.pxDiff || 0} · 글 다름 ${report.textDiff || 0} · 내용 안 보임 ${report.notShown || 0} · 둘 다 빈 화면 ${report.blankBoth || 0}`);
  } finally {
    fs.writeFileSync(path.join(OUT, 'report.json'), JSON.stringify(report, null, 1));
    br.close();
    setTimeout(() => process.exit(process.exitCode || 0), 500);
  }
}

main().catch(e => { console.error(e); process.exit(1); });
