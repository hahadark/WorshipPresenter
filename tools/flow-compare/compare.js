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

const outText = p => p.eval(`(() => { const b = document.body; return b ? b.innerText.replace(/\\s+/g, ' ').trim() : ''; })()`);

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
          for (const o of OUTPUTS) {
            const [ea, ra] = [await pages['el-' + o].shot(), await pages['rs-' + o].shot()];
            const [ei, ri] = [png.decode(ea), png.decode(ra)];
            const d = png.diff(ei, ri);
            const [et, rt] = [await outText(pages['el-' + o]), await outText(pages['rs-' + o])];
            srep[o] = { px: Math.round(d.ratio * 10000) / 100, box: d.box, textSame: et === rt };
            if (et !== rt) srep[o].text = { el: et.slice(0, 300), rs: rt.slice(0, 300) };
            if (d.ratio > 0.004 || et !== rt) {
              const f = path.join(OUT, `${cueId}-${i}-${k}-${o}.png`);
              fs.writeFileSync(f, png.encode(png.sideBySide(ei, ri)));
              srep[o].file = f;
            }
          }
          irep.shots.push(srep);
          const bad = OUTPUTS.filter(o => srep[o].px > 0.4 || !srep[o].textSame);
          if (bad.length) console.log(`   장 ${k + 1}: ` + bad.map(o => `${o} ${srep[o].px}%${srep[o].textSame ? '' : ' 글 다름'}`).join(' · '));
        }
      }
      fs.writeFileSync(path.join(OUT, 'report.json'), JSON.stringify(report, null, 1));
    }
  } finally {
    fs.writeFileSync(path.join(OUT, 'report.json'), JSON.stringify(report, null, 1));
    br.close();
    setTimeout(() => process.exit(process.exitCode || 0), 500);
  }
}

main().catch(e => { console.error(e); process.exit(1); });
