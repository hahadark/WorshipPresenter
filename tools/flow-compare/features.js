// 기능 대조: Electron 판과 Rust 판에 같은 조작을 시키고 출력 3종(그림·글)과 /api/live 를 비교한다.
//   overlays  오버레이(시계·로고·카운트다운·배너·글자) 켜고 끄기 · 모두 끄기/되살리기
//   obs       OBS 자막: 템플릿별 모양 · 그림 우상단(pip) · 슬라이드 그대로(full) — 성경·찬양·찬송가·제목
//   quick     즉석 자막 (짧은 글 · 긴 글 나눔)
//   media     영상 순서(재생·일시정지·끝나면 다음) · 음악 순서(화면 그대로·건너뛰기) · 카메라 순서 · 그림 순서
//   auto      자동 넘김(autoSec · autoLoop · 끝에서 다음 순서) · 슬라이드쇼 · 글자 자동 숨김 · 끝에서 숨김
//   actions   순서의 '송출할 때 자동으로' (타이머 · 무대 메시지 · 로고 · 배경 · 오버레이 · 무대 고정)
//   times     예정 시간
// 준비: node tools\flow-compare\prep.js (두 판을 끈 상태) → launch.js start
// 실행: node tools\flow-compare\features.js --el <포트> --rs-data tmp\rs-data --out tmp\flow-feat [--part overlays,obs,…] [--cue <id>]
'use strict';
const fs = require('fs');
const path = require('path');
const { launch, sleep } = require('./cdp');
const png = require('./png');

const A = process.argv.slice(2);
const opt = (k, d) => { const i = A.indexOf('--' + k); return i >= 0 ? A[i + 1] : d; };
const EL = `http://127.0.0.1:${opt('el', '37528')}`;
const RSD = opt('rs-data', 'tmp/rs-data');
const RS = `http://127.0.0.1:${fs.readFileSync(path.join(RSD, 'flow-port.txt'), 'utf8').trim()}`;
const RSCTL = fs.readFileSync(path.join(RSD, 'flow-ctl.txt'), 'utf8').trim();
const OUT = opt('out', 'tmp/flow-feat');
const OUTPUTS = ['main', 'overlay', 'stage'];
const PART = new Set(opt('part', 'overlays,obs,quick,media,auto,actions,times').split(','));
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
const live = async base => {
  const s = await (await fetch(base + '/api/live')).json(); const p = s.passage || {};
  return { hasLive: !!s.hasLive, visible: !!s.visible, index: s.index, count: s.count, item: p.cueIndex ?? null, title: p.title || '', kind: s.slide ? s.slide.kind || 'bible' : null };
};
// 시계·타이머·카운트다운처럼 흐르는 숫자는 빼고 비교
const outText = p => p.eval(`(() => { const b = document.body; return b ? b.innerText.replace(/\\d{1,2}:\\d{2}(:\\d{2})?/g, '#').replace(/(오전|오후|AM|PM)/g, '').replace(/\\s+/g, ' ').trim() : ''; })()`);

async function main() {
  const br = await launch(path.join(OUT, 'profile'));
  const rows = [];
  let fails = 0;
  try {
    const ctl = await br.open(`${EL}/control/`, 1600, 950);
    if (!(await ctl.waitFor('!!(window.BCCue && window.BC && window.BC.state)', 30000))) throw new Error('Electron 조작 화면이 안 떴습니다');
    const pages = {};
    for (const o of OUTPUTS) { pages['el-' + o] = await br.open(`${EL}/outputs/${o}`); pages['rs-' + o] = await br.open(`${RS}/outputs/${o}`); }
    const cueId = opt('cue', 'cmvtoethn450uc');
    await ctl.eval(`BCCue.openCue(${JSON.stringify(cueId)}).then(() => true)`, 60000); await rs('openCue', { id: cueId });
    let n = (await ctl.eval(`BCCue.cueInfo()`)).items.length;
    if (n !== (await rs('cueItems')).length) throw new Error('두 판의 큐시트 순서 수가 다릅니다');

    // px 는 그림이 다른 화소 비율(%). 시계·카운트다운이 있는 단계는 loose 로 (초가 달라 작은 차이가 난다)
    async function check(name, { loose = 0.4, skipLive = false } = {}) {
      await sleep(250);
      for (const o of OUTPUTS) { await pages['el-' + o].settle(); await pages['rs-' + o].settle(); }
      const row = { step: name };
      const [le, lr] = [await live(EL), await live(RS)];
      row.live = skipLive || JSON.stringify(le) === JSON.stringify(lr) ? 'same' : { el: le, rs: lr };
      for (const o of OUTPUTS) {
        const [ea, ra] = [await pages['el-' + o].shot(), await pages['rs-' + o].shot()];
        const [ei, ri] = [png.decode(ea), png.decode(ra)];
        const d = png.diff(ei, ri);
        const [et, rt] = [await outText(pages['el-' + o]), await outText(pages['rs-' + o])];
        row[o] = { px: Math.round(d.ratio * 10000) / 100, textSame: et === rt };
        if (et !== rt) row[o].text = { el: et.slice(0, 400), rs: rt.slice(0, 400) };
        const f = path.join(OUT, `${String(rows.length).padStart(3, '0')}-${name.replace(/[^\w가-힣]+/g, '_').slice(0, 50)}-${o}.png`);
        if (d.ratio * 100 > loose || et !== rt || A.includes('--keep')) { fs.writeFileSync(f, png.encode(png.sideBySide(ei, ri))); row[o].file = f; }
      }
      rows.push(row);
      const bad = OUTPUTS.filter(o => row[o].px > loose || !row[o].textSame);
      const ok = !bad.length && row.live === 'same';
      if (!ok) fails++;
      console.log(`${ok ? '✓' : '✗'} ${name}` + (row.live !== 'same' ? ` · live E${JSON.stringify(row.live.el)} R${JSON.stringify(row.live.rs)}` : '') + (bad.length ? ' · ' + bad.map(o => `${o} ${row[o].px}%${row[o].textSame ? '' : ' 글 다름'}`).join(' · ') : ''));
      return row;
    }
    const both = async (name, ecmd, eargs, rcmd, rargs, o) => {
      try { await el(ecmd, eargs || {}); } catch (e) { console.log('  E: ' + e.message); }
      try { await rs(rcmd || ecmd, rargs || eargs || {}); } catch (e) { console.log('  R: ' + e.message); }
      return check(name, o);
    };
    const send = async (i, name, o) => {
      let e = null, r = null;
      try { e = await ctl.eval(`BCCue.sendItem(${i})`, 90000); } catch (x) { console.log('  E: ' + x.message.slice(0, 120)); }
      try { r = await rs('sendCueItem', { index: i }); } catch (x) { console.log('  R: ' + x.message.slice(0, 120)); }
      return check(name || `순서 ${i} 송출`, o);
    };
    // 두 판의 큐시트 끝에 같은 순서를 넣는다 → 그 번호
    const add = async item => {
      const { type, ...extra } = item;
      await ctl.eval(`(BCCue.addItem(${JSON.stringify(type)}, ${JSON.stringify({ ...extra, _at: n })}), true)`); await sleep(350);
      await rs('addItem', { item: { id: 'ft' + n, note: '', ...item } });
      return n++;
    };
    const setTpl = async (output, id) => { await el('setTemplate', { output, templateId: id }); await rs('setTemplate', { output, id }); await sleep(400); };
    // 둘 다 같은 상태가 될 때까지 기다린다 (자동 넘김처럼 시각이 조금 어긋나는 것)
    const until = async (pred, ms = 9000) => { const t0 = Date.now(); while (Date.now() - t0 < ms) { const [a, b] = [await live(EL), await live(RS)]; if (pred(a) && pred(b)) return true; await sleep(150); } return false; };
    const BIBLE = Number(opt('bible-item', '6')), SONG = 1, HYMN = 7, TITLE = 0;

    if (PART.has('overlays')) {
      console.log('--- 오버레이');
      await send(BIBLE, '성경 송출');
      for (const [id, name] of [['ovclock1', '시계'], ['ovlogo1', '로고'], ['ovcount1', '카운트다운'], ['ovbanner1', '배너'], ['ovtext1', '글자(OBS만)']]) await both(`오버레이 켬: ${name}`, 'toggleOverlay', { id, on: true }, 'toggleOverlay', { id, on: true }, { loose: 1.5 });
      await both('오버레이 모두 끄기', 'toggleOverlaysOff', {}, 'toggleOverlaysOff', {});
      await both('오버레이 되살리기', 'toggleOverlaysOff', {}, 'toggleOverlaysOff', {}, { loose: 1.5 });
      await both('오버레이 끔: 카운트다운', 'toggleOverlay', { id: 'ovcount1', on: false }, 'toggleOverlay', { id: 'ovcount1', on: false }, { loose: 1.5 });
      await both('다음 슬라이드(오버레이 유지)', 'nextSlide', {}, 'nextSlide', {}, { loose: 1.5 });
      await both('글자 숨김(오버레이는 남음)', 'clearLive', {}, 'clearLive', {}, { loose: 1.5 });
      await both('모두 지우기(오버레이도 꺼짐)', 'clearAll');
      await send(TITLE, '제목 송출(오버레이 꺼진 채)');
      await both('오버레이 되살리기 2', 'toggleOverlaysOff', {}, 'toggleOverlaysOff', {}, { loose: 1.5 });
      for (const id of ['ovclock1', 'ovlogo1', 'ovbanner1', 'ovtext1', 'ovcount1']) { await el('toggleOverlay', { id, on: false }); await rs('toggleOverlay', { id, on: false }); }
      await check('오버레이 모두 끔');
    }

    if (PART.has('obs')) {
      console.log('--- OBS 자막');
      const TPLS = ['theme-bc-clean-lyrics-overlay', 'overlay', 'overlay-lyrics', 'sermon-lower', 'theme-bc-gradient-box-overlay', 'theme-bc-full-band-overlay', 'theme-bc-compact-center-overlay', 'theme-sa-redshield-overlay'];
      for (const t of TPLS) {
        await setTpl('overlay', t);
        for (const [i, nm] of [[BIBLE, '성경'], [SONG, '찬양'], [HYMN, '찬송가'], [TITLE, '제목'], [13, '공지']]) await send(i, `OBS ${t} · ${nm}`);
      }
      await setTpl('overlay', TPLS[0]);
      for (const [i, nm] of [[BIBLE, '성경'], [HYMN, '찬송가'], [TITLE, '제목']]) {
        await send(i, `${nm} 송출`);
        await both(`${nm} · 그림 우상단 끔`, 'setObsView', { pip: false });
        await both(`${nm} · 그림 우상단 켬`, 'setObsView', { pip: true });
        await both(`${nm} · 슬라이드 그대로 켬`, 'setObsView', { full: true });
        await both(`${nm} · 슬라이드 그대로에서 다음`, 'nextSlide');
        await both(`${nm} · 슬라이드 그대로 끔`, 'setObsView', { full: false });
      }
    }

    if (PART.has('quick')) {
      console.log('--- 즉석 자막');
      const q = async (name, text) => {
        try { await ctl.eval(`BCQuick.send(${JSON.stringify(text)})`, 30000); } catch (e) { console.log('  E: ' + e.message.slice(0, 100)); }
        try { await rs('quickText', { text }); } catch (e) { console.log('  R: ' + e.message.slice(0, 100)); }
        return check(name);
      };
      await q('즉석 자막 한 줄', '차량 12가 3456 차주께서는 이동 부탁드립니다');
      await q('즉석 자막 여러 줄', '오늘 예배 후 전교인 식사가 있습니다\n식당으로 오세요\n**새가족 환영회** 2층 소예배실');
      await q('즉석 자막 긴 글(자동 나눔)', Array.from({ length: 14 }, (_, i) => `${i + 1}번째 줄입니다. 광고 내용을 길게 적어서 한 장에 다 들어가지 않게 만든 글입니다.`).join('\n'));
      await both('즉석 자막 다음 장', 'nextSlide');
      await both('즉석 자막 다음 장 2', 'nextSlide');
      await q('즉석 자막 --- 나눔', '첫 장\n---\n둘째 장\n---\n셋째 장');
      await both('즉석 자막 끝에서 다음 x3', 'nextSlide'); await both('·', 'nextSlide'); await both('··', 'nextSlide');
      await q('즉석 자막 교독 역할', '인도자: 여호와께 감사하라\n회중: 그는 선하시며 그 인자하심이 영원함이로다');
    }

    if (PART.has('media')) {
      console.log('--- 영상·음악·카메라·그림 순서');
      const before = await add({ type: 'title', title: '영상 앞 순서' });
      const vid = await add({ type: 'video', title: '시험 영상', file: '/media/flowtest.mp4', sound: false, loop: true, endAction: 'hold' });
      const aud = await add({ type: 'audio', title: '시험 음악', file: '/media/flowtest.mp3', volume: 0.2 });
      const img = await add({ type: 'image', title: '시험 그림', image: '/media/flowimg1.png' });
      const cam = await add({ type: 'camera', title: '없는 카메라', deviceId: 'no-such-device', label: '없는 카메라' });
      const vnext = await add({ type: 'video', title: '짧은 영상(끝나면 다음)', file: '/media/flowshort.mp4', sound: false, endAction: 'next' });
      const after = await add({ type: 'title', title: '짧은 영상 다음 순서' });
      const vblack = await add({ type: 'video', title: '짧은 영상(끝나면 검은 화면)', file: '/media/flowshort.mp4', sound: false, endAction: 'black' });
      await add({ type: 'blank', title: '끝' });
      for (const [nm, i] of [['영상', vid], ['음악', aud], ['그림', img], ['카메라', cam]]) {
        const [es, rd] = [await ctl.eval(`BCCue.slidesFor(${i})`).catch(e => e.message), await rs('slidesFor', { index: i }).catch(e => e.message)];
        const [a, b] = [JSON.stringify((es || []).map ? es.map(s => [s.kind, s.label, s.image || '', s.video || null, s.camera || null]) : es), JSON.stringify(rd.slides ? rd.slides.map(s => [s.kind, s.label, s.image || '', s.video || null, s.camera || null]) : rd)];
        console.log(`${a === b ? '✓' : '✗'} ${nm} 순서 슬라이드 모양${a === b ? '' : '\n   E ' + a.slice(0, 300) + '\n   R ' + b.slice(0, 300)}`); if (a !== b) fails++;
      }
      await send(before, '영상 앞 순서 송출');
      await send(vid, '영상 송출'); await sleep(2500); await check('영상 재생 중(2.5초 뒤)');
      await both('영상 일시정지', 'videoControl', { action: 'pause' }, 'videoControl', { action: 'pause' });
      await both('영상 다시 재생', 'videoControl', { action: 'play' }, 'videoControl', { action: 'play' });
      await both('영상에서 글자 숨김', 'toggleLive'); await both('영상 다시 보임', 'toggleLive');
      await both('영상에서 다음 → 음악은 건너뛰고 그림', 'nextSlide');
      await both('그림에서 이전 → 음악 건너뛰고 영상', 'previousSlide');
      await send(aud, '음악 순서 송출(화면은 그대로)');
      await send(img, '그림 송출'); await send(cam, '카메라 송출');
      await send(vnext, '짧은 영상(끝나면 다음) 송출');
      console.log((await until(l => l.item === after, 12000)) ? '✓ 영상이 끝나 다음 순서로 넘어감' : (fails++, '✗ 영상이 끝난 뒤 다음 순서로 가지 않음: ' + JSON.stringify([await live(EL), await live(RS)])));
      await check('짧은 영상 뒤');
      await send(vblack, '짧은 영상(끝나면 검은 화면) 송출'); await sleep(6500); await check('영상이 끝난 뒤(검은 화면)');
    }

    if (PART.has('auto')) {
      console.log('--- 자동 넘김');
      const ss = await add({ type: 'slideshow', title: '예배 전 화면', images: ['/media/flowimg1.png', '/media/flowimg2.png'], body: '광고 하나\n---\n광고 둘', autoSec: 3, autoLoop: true, countdown: '23:59', countdownLabel: '예배 시작까지' });
      const at = await add({ type: 'text', title: '자동 넘김 공지', body: '첫째\n---\n둘째\n---\n셋째', autoSec: 2 });
      const nx = await add({ type: 'title', title: '자동 넘김 다음 순서' });
      const hd = await add({ type: 'text', title: '자동 숨김', body: '2초 뒤에 글자만 숨는 공지', hideAfter: 2 });
      const eh = await add({ type: 'text', title: '끝에서 숨김', body: '하나\n---\n둘', edgeHide: true });
      await add({ type: 'blank', title: '끝' });
      await send(ss, '슬라이드쇼 송출', { loose: 1.5 });
      for (const k of [1, 2, 3, 0, 1]) { const ok = await until(l => l.item === ss && l.index === k, 8000); console.log(`${ok ? '✓' : '✗'} 슬라이드쇼 자동 넘김 → ${k + 1}번째 장`); if (!ok) { fails++; console.log('   ' + JSON.stringify([await live(EL), await live(RS)])); break; } await check(`슬라이드쇼 ${k + 1}번째 장`, { loose: 1.5 }); }
      await both('슬라이드쇼에서 글자 숨김(자동 넘김 멈춤)', 'clearLive', {}, 'clearLive', {}, { loose: 1.5 });
      await sleep(4500); await check('숨긴 채 4.5초 뒤(그대로)', { loose: 1.5 });
      await send(at, '자동 넘김 공지 송출');
      for (const k of [1, 2]) { const ok = await until(l => l.item === at && l.index === k, 6000); console.log(`${ok ? '✓' : '✗'} 공지 자동 넘김 → ${k + 1}번째 장`); if (!ok) fails++; }
      const ok2 = await until(l => l.item === nx, 6000); console.log(`${ok2 ? '✓' : '✗'} 마지막 장 뒤 다음 순서로 자동`); if (!ok2) { fails++; console.log('   ' + JSON.stringify([await live(EL), await live(RS)])); }
      await check('자동으로 넘어간 다음 순서');
      await send(hd, '자동 숨김 공지 송출');
      const ok3 = await until(l => l.item === hd && l.visible === false, 6000); console.log(`${ok3 ? '✓' : '✗'} 2초 뒤 글자 숨김`); if (!ok3) fails++;
      await check('자동으로 숨은 뒤');
      await both('숨은 뒤 다음(다시 보이며 넘어감)', 'nextSlide');
      await send(eh, '끝에서 숨김 공지 송출');
      await both('끝에서 숨김: 다음', 'nextSlide'); await both('끝에서 숨김: 마지막 장에서 다음 → 숨김', 'nextSlide'); await both('끝에서 숨김: 한 번 더 → 다음 순서', 'nextSlide');
    }

    if (PART.has('actions')) {
      console.log('--- 송출할 때 자동으로');
      const a1 = await add({ type: 'title', title: '자동: 타이머·무대 메시지·로고', actions: { timer: 5, stageMsg: '자동 무대 메시지', logo: 'on' } });
      const a2 = await add({ type: 'title', title: '자동: 로고 끔·배경 숨김·배너 켬·무대 메시지 지움', actions: { logo: 'off', bg: 'hide', ovOn: 'ovbanner1', clearStage: true } });
      const a3 = await add({ type: 'title', title: '자동: 배경 보임·오버레이 모두 끔·무대 고정(설교 요점)', actions: { bg: 'show', ovOff: '*', stagePin: 'sermon1' } });
      const a4 = await add({ type: 'title', title: '자동: 무대 순서표', actions: { stagePin: 'order' } });
      const a5 = await add({ type: 'title', title: '자동: 무대 고정 풂·시계 켬·다른 것 끔', actions: { stagePin: 'live', ovOn: 'ovclock1', ovOff: '*' } });
      await send(BIBLE, '성경 송출(자동 동작 앞)');
      await send(a1, '자동 1 송출'); await send(a2, '자동 2 송출', { loose: 1.5 }); await send(a3, '자동 3 송출'); await send(a4, '자동 4 송출'); await send(a5, '자동 5 송출', { loose: 1.5 });
      await both('다음으로 넘어갈 때는 자동 동작 없음(이전)', 'previousSlide', {}, 'previousSlide', {}, { loose: 1.5 });
      await el('stageTimer', { action: 'reset' }); await rs('stageTimer', { op: 'reset' }); await el('toggleOverlay', { id: 'ovclock1', on: false }); await rs('toggleOverlay', { id: 'ovclock1', on: false });
      await check('자동 동작 정리');
    }

    if (PART.has('times')) {
      console.log('--- 예정 시간');
      const e = await ctl.eval(`(() => { const hm = m => String(Math.floor(m / 60) % 24).padStart(2, '0') + ':' + String(Math.round(m) % 60).padStart(2, '0'); const c = BCCue.cueInfo(), meta = BCCue.meta(); const mm = /^(\\d{1,2}):(\\d{2})$/.exec(meta.startTime || ''); if (!mm) return { start: meta.startTime, shown: [...document.querySelectorAll('.cue-time')].map(x => x.firstChild.textContent) }; let t = +mm[1] * 60 + +mm[2]; return { start: meta.startTime, times: c.items.map(it => { const at = t; t += Number(it.duration) || 0; return hm(at); }), shown: [...document.querySelectorAll('.cue-time')].map(x => x.firstChild.textContent) }; })()`);
      const r = await rs('plannedTimes');
      const same = JSON.stringify(e.times || []) === JSON.stringify((r || []).filter(Boolean).length ? r : []);
      console.log(`${same ? '✓' : '✗'} 예정 시간 (시작 ${e.start || '없음'}): E ${JSON.stringify((e.times || []).slice(0, 8))} R ${JSON.stringify((r || []).slice(0, 8))}`); if (!same) fails++;
    }
    console.log(`== 다른 단계 ${fails}개 / 확인 ${rows.length}단계`);
  } finally {
    fs.writeFileSync(path.join(OUT, `report-${opt('part', 'all').replace(/,/g, '_')}.json`), JSON.stringify(rows, null, 1));
    br.close();
    setTimeout(() => process.exit(0), 500);
  }
}
main().catch(e => { console.error(e); process.exit(1); });
