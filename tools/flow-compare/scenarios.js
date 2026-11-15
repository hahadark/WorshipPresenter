// 조작 흐름 대조: 지우기·배경·로고·회중 메시지·무대 메시지/타이머·순서 끝 넘기기·성경 나눔을
// Electron 판과 Rust 판에 똑같이 시키고, 단계마다 출력 3종의 그림·글과 /api/live 를 비교한다.
// 실행: node tools\flow-compare\scenarios.js --el 37530 --rs-data tmp\rs-data --out tmp\flow-sc --cue <id> [--bible-item 6]
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
const OUT = opt('out', 'tmp/flow-sc');
const OUTPUTS = ['main', 'overlay', 'stage'];
fs.mkdirSync(OUT, { recursive: true });

async function rs(cmd, args = {}) {
  const r = await (await fetch(RSCTL, { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ cmd, args }) })).json();
  if (!r.ok) throw new Error(`Rust ${cmd}: ${r.error}`);
  return r.result;
}
async function el(cmd, args = {}) {
  const r = await (await fetch(EL + '/api/command', { method: 'POST', headers: { 'content-type': 'application/json', 'x-worshipcue': '1' }, body: JSON.stringify({ cmd, args }) })).json();
  if (r.ok === false) throw new Error(`Electron ${cmd}: ${r.error}`);
  return r.result;
}
const live = async base => { const s = await (await fetch(base + '/api/live')).json(); const p = s.passage || {};
  return { hasLive: !!s.hasLive, visible: !!s.visible, index: s.index, count: s.count, item: p.cueIndex ?? (p.cue && p.cue.index) ?? null, title: p.title || '',
    slide: s.slide ? (s.slide.kind || '') + ':' + (s.slide.ref || s.slide.title || '') + ':' + ((s.slide.parts || []).map(v => `${v.chapter}:${v.verse}${v.partCount ? '#' + v.partIndex + '/' + v.partCount : ''}`).join(',')) : null }; };
// 시계·타이머처럼 흐르는 글은 빼고 비교
const outText = p => p.eval(`(() => { const b = document.body; return b ? b.innerText.replace(/\\d{1,2}:\\d{2}(:\\d{2})?/g, '#').replace(/(오전|오후|AM|PM)/g, '').replace(/\\s+/g, ' ').trim() : ''; })()`);

async function main() {
  const br = await launch(path.join(OUT, 'profile'));
  const rows = [];
  try {
    const ctl = await br.open(`${EL}/control/`, 1600, 950);
    if (!(await ctl.waitFor('!!(window.BCCue && window.BC && window.BC.state)', 30000))) throw new Error('Electron 조작 화면이 안 떴습니다');
    const pages = {};
    for (const o of OUTPUTS) { pages['el-' + o] = await br.open(`${EL}/outputs/${o}`); pages['rs-' + o] = await br.open(`${RS}/outputs/${o}`); }
    const cueId = opt('cue', await ctl.eval(`BCCue.currentId()`));
    await ctl.eval(`BCCue.openCue(${JSON.stringify(cueId)})`); await rs('openCue', { id: cueId });
    const n0 = (await ctl.eval(`BCCue.cueInfo()`)).items.length;
    const bi = Number(opt('bible-item', '6'));

    async function check(name) {
      await sleep(200);
      for (const o of OUTPUTS) { await pages['el-' + o].settle(); await pages['rs-' + o].settle(); }
      const row = { step: name };
      const [le, lr] = [await live(EL), await live(RS)];
      row.live = JSON.stringify(le) === JSON.stringify(lr) ? 'same' : { el: le, rs: lr };
      for (const o of OUTPUTS) {
        const [ea, ra] = [await pages['el-' + o].shot(), await pages['rs-' + o].shot()];
        const [ei, ri] = [png.decode(ea), png.decode(ra)];
        const d = png.diff(ei, ri);
        const [et, rt] = [await outText(pages['el-' + o]), await outText(pages['rs-' + o])];
        row[o] = { px: Math.round(d.ratio * 10000) / 100, textSame: et === rt };
        if (et !== rt) row[o].text = { el: et.slice(0, 400), rs: rt.slice(0, 400) };
        if (d.ratio > 0.004 || et !== rt || A.includes('--keep')) { const f = path.join(OUT, `${String(rows.length).padStart(2, '0')}-${name.replace(/[^\w가-힣]+/g, '_')}-${o}.png`); fs.writeFileSync(f, png.encode(png.sideBySide(ei, ri))); row[o].file = f; }
      }
      rows.push(row);
      const bad = OUTPUTS.filter(o => row[o].px > 0.4 || !row[o].textSame);
      console.log(`${bad.length || row.live !== 'same' ? '✗' : '✓'} ${name}` + (row.live !== 'same' ? ` · live E${JSON.stringify(row.live.el)} R${JSON.stringify(row.live.rs)}` : '') + (bad.length ? ' · ' + bad.map(o => `${o} ${row[o].px}%${row[o].textSame ? '' : ' 글 다름'}`).join(' · ') : ''));
    }
    const both = async (name, ecmd, eargs, rcmd, rargs) => {
      try { await el(ecmd, eargs || {}); } catch (e) { console.log('  E오류 ' + e.message); }
      try { await rs(rcmd || ecmd, rargs || eargs || {}); } catch (e) { console.log('  R오류 ' + e.message); }
      await check(name);
    };
    const sendItem = async (i, name) => { await ctl.eval(`BCCue.sendItem(${i})`, 90000); await rs('sendItem', { index: i }); await check(name || `순서 ${i} 송출`); };

    const PART = new Set(opt('part', 'ops,edge,split').split(','));
    if (PART.has('ops')) {
    // ---- 1. 지우기·배경·로고 ----
    await sendItem(bi, '성경 순서 송출');
    await both('다음', 'nextSlide'); await both('다음2', 'nextSlide'); await both('이전', 'previousSlide');
    await both('글자 숨김(clearLive)', 'clearLive'); await both('숨긴 채 다음', 'nextSlide'); await both('다시 보임(restoreLive)', 'restoreLive');
    await both('F10 토글 끔', 'toggleLive'); await both('F10 토글 켬', 'toggleLive');
    await both('배경 끄기', 'toggleBackground', {}, 'toggleBg'); await both('배경 켜기', 'toggleBackground', {}, 'toggleBg');
    await both('로고 화면 켬', 'toggleLogoScreen', {}, 'toggleLogo'); await both('로고 화면 끔', 'toggleLogoScreen', {}, 'toggleLogo');
    // ---- 2. 회중 메시지 · 무대 ----
    await both('회중 메시지(예배당+OBS)', 'showMessage', { text: '차량 12가 3456 이동 부탁드립니다', outputs: ['main', 'overlay'], seconds: 0 });
    await both('회중 메시지 지움', 'clearMessage');
    await both('회중 메시지(예배당만)', 'showMessage', { text: '유아실 보호자 호출', outputs: ['main'], seconds: 0 });
    await both('회중 메시지 지움2', 'clearMessage');
    await both('무대 메시지', 'stageMessage', { text: '마이크 3번 켜 주세요' });
    await both('무대 타이머 시작', 'stageTimer', { action: 'start', minutes: 5 }, 'stageTimer', { op: 'start', minutes: 5 });
    await both('무대 타이머 멈춤', 'stageTimer', { action: 'pause' }, 'stageTimer', { op: 'pause' });
    await both('무대 타이머 초기화', 'stageTimer', { action: 'reset' }, 'stageTimer', { op: 'reset' });
    await both('무대 메시지 지움', 'stageMessage', { text: '' });
    // ---- 3. 모두 지우기 → 다시 송출 ----
    await both('모두 지우기', 'clearAll');
    await sendItem(bi, '모두 지운 뒤 다시 송출');
    }
    // ---- 4. 순서 끝·처음에서 넘기기 (--edge-item: 앞뒤가 평범한 순서인 것) ----
    if (PART.has('edge')) {
      const ei = Number(opt('edge-item', '13'));
      await sendItem(ei, '순서 송출(끝 넘기기용)');
      await both('첫 장에서 이전 → 앞 순서 마지막 장', 'previousSlide');
      await both('다음 → 원래 순서 첫 장', 'nextSlide');
      const cnt = (await live(EL)).count || 1;
      for (let k = 1; k < cnt; k++) { await el('nextSlide').catch(() => {}); await rs('nextSlide'); }
      await check('마지막 장');
      await both('마지막 장에서 다음 → 다음 순서', 'nextSlide');
      await both('다음(그다음 순서)', 'nextSlide');
      await both('다음(큐시트 끝 근처)', 'nextSlide');
      await both('다음(큐시트 끝에서 한 번 더)', 'nextSlide');
      await both('다음(끝에서 또)', 'nextSlide');
      await sendItem(0, '첫 순서 송출');
      await both('큐시트 첫 순서에서 이전', 'previousSlide');
    }
    if (PART.has('split')) {
    // ---- 5. 성경 나눔: 긴 절·여러 장 걸침·병행 번역, 나눔 방식별 ----
    // --parallel <번역본 id>: 병행 번역을 켜고 비교 · --refs 'a|b': 볼 장절
    const par = opt('parallel', '');
    await el('updateSettings', { patch: { parallelVersionId: par } }); await rs('updateSettings', { patch: { parallelVersionId: par } });
    await ctl.eval(`(BC.state.settings.parallelVersionId = ${JSON.stringify(par)}, true)`);
    const refs = opt('refs', '') ? opt('refs', '').split('|') : ['에 8:9', '롬 8:28-39', '창 1:26-2:3', '시 119:1-16', '요 3:16-21', '단 5:23', '렘 44:17-19'];
    const eq = [];
    let at = n0;
    for (const split of [{ mode: 'count', count: 1 }, { mode: 'count', count: 2 }, { mode: 'auto', count: 2 }]) {
      await el('updateSettings', { patch: { split } }); await rs('updateSettings', { patch: { split } });
      await ctl.eval(`(BC.state.settings.split = ${JSON.stringify(split)}, true)`);
      for (const ref of (split.mode === 'count' && split.count === 2 ? refs.slice(1, 3) : refs)) {
        const item = { type: 'bible', title: ref, ref, note: '' };
        await ctl.eval(`(BCCue.addItem('bible', ${JSON.stringify({ title: ref, ref, _at: at })}), true)`); await sleep(400);
        await rs('addItem', { item: { id: 'fc' + at, ...item } });
        let es = [], rd = { slides: [] }, err = '';
        try { es = await ctl.eval(`BCCue.slidesFor(${at})`, 60000); } catch (e) { err += ' E:' + e.message.slice(0, 80); }
        try { rd = await rs('slidesFor', { index: at }); } catch (e) { err += ' R:' + e.message.slice(0, 80); }
        const key = s => (s.verses || s.parts || []).map(v => `${v.chapter}:${v.verse}${v.partCount ? '#' + (v.partIndex + 1) + '/' + v.partCount : ''}:${(v.text || '').length}`).join(',');
        const ek = es.map(key), rk = rd.slides.map(key);
        const same = JSON.stringify(ek) === JSON.stringify(rk);
        eq.push({ split: split.mode + split.count, ref, el: es.length, rs: rd.slides.length, same, err, ek: same ? undefined : ek, rk: same ? undefined : rk });
        console.log(`${same ? '✓' : '✗'} 나눔 ${split.mode}${split.count} ${ref}: E${es.length}/R${rd.slides.length}${err}`);
        if (es.length && rd.slides.length) { await sendItem(at, `나눔 ${split.mode}${split.count} ${ref} 첫 장`);
          if (Math.min(es.length, rd.slides.length) > 1) { await both(`나눔 ${split.mode}${split.count} ${ref} 둘째 장`, 'nextSlide'); } }
        at++;
      }
    }
    fs.writeFileSync(path.join(OUT, 'split.json'), JSON.stringify(eq, null, 1));
    }
  } finally {
    fs.writeFileSync(path.join(OUT, 'report-' + opt('part', 'all').replace(/,/g, '_') + '.json'), JSON.stringify(rows, null, 1));
    br.close();
    setTimeout(() => process.exit(0), 500); // 열린 웹소켓 때문에 안 끝나는 것 막기
  }
}
main().catch(e => { console.error(e); process.exit(1); });
