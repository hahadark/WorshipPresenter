// 찬송가·PPT 출력 대조: Electron 판과 Rust 판의 같은 큐시트 끝에 같은 찬송가·교독문·붙여 넣은 PPT 순서를 넣고,
// 순서마다 슬라이드 장 수·내용(meta)과 웹 출력(/outputs/main · overlay)을 1920×1080 으로 찍어 비교한다.
//
// 준비는 README.md 와 같다 (자료 두 벌 · Electron 소스 실행 · flow_driver).
// 실행: node tools\flow-compare\hymns.js --el <포트> --rs-data tmp\rs-data --el-data tmp\el-data --out tmp\hymn-out --files files.json
//   files.json: [{ "title": "3장 성부 성자와 성령", "file": "D:\\…\\003 ….pps", "number": 3 }, { "pasted": "clip-….pptx" }, …]
//   "pasted" 는 두 자료 폴더의 pasted\ 아래 같은 이름 파일 (판마다 자기 폴더 것만 열 수 있다)
//   순서에 넣을 다른 값(background · obsLyrics 등)은 "extra": {…} 로.
'use strict';
const fs = require('fs');
const path = require('path');
const { launch, sleep } = require('./cdp');
const png = require('./png');

const A = process.argv.slice(2);
const opt = (k, d) => { const i = A.indexOf('--' + k); return i >= 0 ? A[i + 1] : d; };
const EL = `http://127.0.0.1:${opt('el', '37528')}`;
const RSD = path.resolve(opt('rs-data', 'tmp/rs-data'));
const ELD = path.resolve(opt('el-data', 'tmp/el-data'));
const RS = `http://127.0.0.1:${fs.readFileSync(path.join(RSD, 'flow-port.txt'), 'utf8').trim()}`;
const RSCTL = fs.readFileSync(path.join(RSD, 'flow-ctl.txt'), 'utf8').trim();
const OUT = opt('out', 'tmp/hymn-out');
const MAXS = Number(opt('max-slides', '6'));
const OUTPUTS = opt('outputs', 'main,overlay').split(',');
const FILES = JSON.parse(fs.readFileSync(opt('files', 'files.json'), 'utf8').replace(/^\ufeff/, ''));
fs.mkdirSync(OUT, { recursive: true });

async function rs(cmd, args = {}) {
  const r = await (await fetch(RSCTL, { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ cmd, args }) })).json();
  if (!r.ok) throw new Error(r.error);
  return r.result;
}

// 캐시 열쇠·자료 폴더가 판마다 달라서 주소에서 뺀다
const norm = s => JSON.stringify(s, (k, v) => (typeof v === 'string' ? v.replace(/\/hymn\/[0-9a-f]{16}\//g, '/hymn/K/').replace(/\?v=[^"&]*/g, '') : v), 0);
const keysOf = s => {
  const o = JSON.parse(norm(s));
  for (const k of ['id', 'itemId', 'cueId', 'key', 'file']) delete o[k];
  // 키 차례만 다른 것은 같은 것으로
  const sort = v => (Array.isArray(v) ? v.map(sort) : v && typeof v === 'object' ? Object.fromEntries(Object.keys(v).sort().map(k => [k, sort(v[k])])) : v);
  return sort(o);
};
const outText = p => p.eval(`(() => { const b = document.body; return b ? b.innerText.replace(/\\s+/g, ' ').trim() : ''; })()`);

async function main() {
  const br = await launch(path.join(OUT, 'profile'));
  const report = { started: new Date().toISOString(), items: [] };
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
    let info = await ctl.eval(`BCCue.cueInfo()`);
    const cueId = opt('cue', info && info.id);
    await ctl.eval(`BCCue.openCue(${JSON.stringify(cueId)})`);
    await rs('openCue', { id: cueId });
    for (const [n, f] of FILES.entries()) {
      const tag = String(n + 1).padStart(2, '0');
      const base = { background: 'default', obsLyrics: '', ...(f.extra || {}) };
      const eItem = { title: f.title || f.pasted, number: f.number ?? null, file: f.pasted ? path.join(ELD, 'pasted', f.pasted) : f.file, ...base };
      const rItem = { type: 'hymn', ...eItem, file: f.pasted ? path.join(RSD, 'pasted', f.pasted) : f.file };
      const irep = { n: n + 1, title: eItem.title, file: path.basename(eItem.file), extra: f.extra || null };
      report.items.push(irep);
      // 넣기
      await ctl.eval(`BCCue.addItem('hymn', ${JSON.stringify({ ...eItem, _at: 100000 })})`, 120000).catch(e => { irep.elAdd = e.message; });
      const ri = await rs('addItem', { item: rItem });
      info = await ctl.eval(`BCCue.cueInfo()`);
      const ei = info.items.length - 1;
      // 슬라이드 (처음 만드는 시간도 잰다)
      let es = [], rsl = [];
      let t0 = Date.now();
      try { es = await ctl.eval(`BCCue.slidesFor(${ei})`, 180000); } catch (e) { irep.elError = e.message; }
      irep.elMs = Date.now() - t0;
      t0 = Date.now();
      for (let tries = 0; tries < 240; tries++) {
        try { rsl = (await rs('slidesFor', { index: ri })).slides; irep.rsError = undefined; break; } catch (e) {
          irep.rsError = e.message;
          if (!/만드는 중|being (made|built|rendered)/i.test(e.message)) break;
          await sleep(250);
        }
      }
      irep.rsMs = Date.now() - t0;
      irep.count = [es.length, rsl.length];
      irep.diffs = [];
      for (let k = 0; k < Math.max(es.length, rsl.length); k++) {
        const [a, b] = [es[k] ? keysOf(es[k]) : null, rsl[k] ? keysOf(rsl[k]) : null];
        if (JSON.stringify(a) === JSON.stringify(b)) continue;
        const ks = [...new Set([...Object.keys(a || {}), ...Object.keys(b || {})])].filter(x => JSON.stringify((a || {})[x]) !== JSON.stringify((b || {})[x]));
        irep.diffs.push({ slide: k, keys: ks, el: Object.fromEntries(ks.map(x => [x, JSON.stringify((a || {})[x] ?? null).slice(0, 240)])), rs: Object.fromEntries(ks.map(x => [x, JSON.stringify((b || {})[x] ?? null).slice(0, 240)])) });
      }
      console.log(`[${tag}] ${irep.title.slice(0, 28)}: 장 수 E${es.length}/R${rsl.length} · 처음 E${irep.elMs}ms/R${irep.rsMs}ms${irep.diffs.length ? ' · 내용 차이 ' + irep.diffs.length + '장 (' + [...new Set(irep.diffs.flatMap(d => d.keys))].join(',') + ')' : ''}${irep.elError ? ' · E오류 ' + irep.elError : ''}${irep.rsError ? ' · R오류 ' + irep.rsError : ''}`);
      if (!es.length || !rsl.length) continue;
      await ctl.eval(`BCCue.sendItem(${ei})`, 120000);
      await rs('sendItem', { index: ri });
      irep.shots = [];
      const cnt = Math.min(es.length, rsl.length, MAXS);
      for (let k = 0; k < cnt; k++) {
        if (k > 0) { await ecmd('nextSlide'); await rs('nextSlide'); }
        await sleep(k === 0 ? 3500 : 2500); // OBS 자막이 서서히 바뀌는 동안 찍으면 상자 진하기가 달라 보인다
        // 머리 없는 브라우저는 뒤에 있는 탭의 전환 효과를 멈춰 둔다 — 한 번씩 찍어 깨운 뒤 다시 찍는다 (안 그러면 OBS 자막이 흐리게 찍힌다)
        for (const o of OUTPUTS) { await pages['el-' + o].shot(); await pages['rs-' + o].shot(); }
        await sleep(1500);
        for (const o of OUTPUTS) { await pages['el-' + o].shot(); await pages['rs-' + o].shot(); }
        await sleep(800);
        const srep = { slide: k };
        for (const o of OUTPUTS) {
          const [ea, ra] = [await pages['el-' + o].shot(), await pages['rs-' + o].shot()];
          const [ei2, ri2] = [png.decode(ea), png.decode(ra)];
          const d = png.diff(ei2, ri2);
          const [et, rt] = [await outText(pages['el-' + o]), await outText(pages['rs-' + o])];
          srep[o] = { px: Math.round(d.ratio * 10000) / 100, box: d.box, textSame: et === rt };
          if (et !== rt) srep[o].text = { el: et.slice(0, 300), rs: rt.slice(0, 300) };
          // 첫 장은 늘 남기고, 나머지는 다를 때만
          if (k === 0 || d.ratio > 0.004 || et !== rt) {
            const file = path.join(OUT, `${tag}-${k + 1}-${o}.png`);
            fs.writeFileSync(file, png.encode(png.sideBySide(ei2, ri2)));
            srep[o].file = file;
          }
        }
        irep.shots.push(srep);
        const bad = OUTPUTS.filter(o => srep[o].px > 0.4 || !srep[o].textSame);
        if (bad.length) console.log(`   장 ${k + 1}: ` + bad.map(o => `${o} ${srep[o].px}%${srep[o].textSame ? '' : ' 글 다름'}`).join(' · '));
      }
      fs.writeFileSync(path.join(OUT, 'report.json'), JSON.stringify(report, null, 1));
    }
  } finally {
    fs.writeFileSync(path.join(OUT, 'report.json'), JSON.stringify(report, null, 1));
    br.close();
  }
}

main().catch(e => { console.error(e); process.exit(1); });
