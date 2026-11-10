'use strict';
// 조작 화면 점검 (run.js 가 Electron 으로 띄운다): 숨은 창에 /control/ 을 열고 실제 화면 상태를 본다
const { app, BrowserWindow } = require('electron');
const fs = require('fs');
const path = require('path');
app.commandLine.appendSwitch('use-fake-device-for-media-stream'); // 카메라 점검용 가짜 카메라
app.commandLine.appendSwitch('use-fake-ui-for-media-stream');
const [port, OUT] = process.argv.slice(-2);
const wait = ms => new Promise(r => setTimeout(r, ms));
const R = { log: [], fails: [], console: [] };
const ok = (cond, name, detail) => { if (cond) R.log.push('✓ ' + name); else R.fails.push(name + (detail ? ` — ${typeof detail === 'string' ? detail : JSON.stringify(detail)}` : '')); };

app.on('window-all-closed', () => {});
app.whenReady().then(async () => {
  let w;
  try {
    w = new BrowserWindow({ width: 1600, height: 1000, show: false, webPreferences: { offscreen: true } });
    w.webContents.on('console-message', e => { if (e.level === 'error' || /Uncaught|TypeError|ReferenceError/.test(e.message)) R.console.push(e.message.slice(0, 300)); });
    const ev = c => w.webContents.executeJavaScript(`(async () => { const wait = ms => new Promise(r => setTimeout(r, ms)); const $ = s => document.querySelector(s); const $$ = s => [...document.querySelectorAll(s)]; try { ${c} } catch (e) { return { jsErr: String(e.stack).slice(0, 400) }; } })()`);
    const shot = name => w.webContents.capturePage().then(i => fs.writeFileSync(path.join(OUT, name + '.png'), i.toPNG()));
    // 화면이 스스로 다시 읽으면(언어 바꿈 등) loadURL 이 ERR_ABORTED(-3) 로 끝날 수 있다 → 잠깐 뒤 다시
    const load = async () => { for (let k = 0; ; k++) { try { await w.loadURL(`http://127.0.0.1:${port}/control/`); break; } catch (e) { if (k >= 3 || !/\(-3\)|ERR_ABORTED/.test(String(e && e.message))) throw e; await wait(1500); } } await wait(3500); await ev(`if (!$('#modal').hidden) $('#modalClose').click(); return 1;`); };
    await load();

    // ---- 1. 화면이 뜨고 기본 요소가 있다
    const basics = await ev(`return { q: !!$('#q'), cue: !!$('#cueList'), pv: !!$('#pvScreen'), lv: !!$('#lvScreen'), ver: $('#version') && $('#version').options.length };`);
    ok(basics.q && basics.cue && basics.pv && basics.lv, '조작 화면 기본 요소', basics);
    ok(basics.ver >= 1, '성경 번역본 목록', basics);

    // ---- 2. 성경: 검색 → 미리보기 → 송출 → 지우기
    const bible = await ev(`$('#q').focus(); $('#q').value = '요 3:16'; $('#q').dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true })); await wait(1500);
      const pv = $('#pvScreen').innerText.replace(/\\s+/g, ' ');
      await window.BC.sendPreview(); await wait(800);
      const lv = $('#lvScreen').innerText.replace(/\\s+/g, ' ');
      await window.BC.cmd('clearAll', {}); await wait(600);
      return { pv, lv, after: window.BC.state.live.visible };`);
    ok(/하나님이 세상을/.test(bible.pv || ''), '성경 검색 → 미리보기', bible);
    ok(/하나님이 세상을/.test(bible.lv || ''), '송출 (F9)', bible);
    ok(bible.after === false, '모두 지우기', bible);

    // ---- 2-1. 즉석 자막: Ctrl+T → 글 쓰고 Enter → 바로 송출 → 최근 목록 → Esc 로 닫기 (보조 화면은 그대로)
    const qt = await ev(`document.body.focus(); window.dispatchEvent(new KeyboardEvent('keydown', { key: 't', code: 'KeyT', ctrlKey: true, bubbles: true })); await wait(300);
      const opened = window.BCQuick.isOpen() && document.activeElement === $('#qtInput');
      $('#qtInput').value = '예배 후 전 교인 식사가 있습니다';
      $('#qtInput').dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', code: 'Enter', bubbles: true, cancelable: true })); await wait(900);
      const lv = $('#lvScreen').innerText.replace(/\\s+/g, ' ');
      const title = window.BC.state.live && window.BC.state.live.passage && window.BC.state.live.passage.title;
      const list = $$('#qtList .qt-send').map(b => b.textContent);
      return { opened, lv, title, list };`);
    await shot('quicktext');
    Object.assign(qt, await ev(`$('#qtInput').dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', code: 'Escape', bubbles: true })); await wait(200);
      const closed = !window.BCQuick.isOpen();
      await window.BC.cmd('clearAll', {}); await wait(300);
      return { closed, input: $('#qtInput').value };`));
    ok(qt.opened && qt.closed, '즉석 자막: Ctrl+T 로 열고 Esc 로 닫기', qt);
    ok(/전 교인 식사/.test(qt.lv || '') && qt.title === '즉석 자막' && qt.input === '', '즉석 자막: Enter 로 바로 송출', qt);
    ok(qt.list && qt.list[0] === '예배 후 전 교인 식사가 있습니다', '즉석 자막: 최근 목록에 남음', qt);

    // ---- 2-2. 교독문 역할: '회중:' 줄은 굵게, 머리말은 빠짐 → '역할 이름도 보이기' 를 켜면 머리말이 화면에
    const rd = await ev(`const c = (await window.BC.cmd('saveCue', { cue: { name: '교독문 점검', items: [{ type: 'text', title: '교독문 1', body: '인도자: 복 있는 사람은\\n회중: 악인들의 꼬임을 따르지 아니하며' }] } })).result;
      window.BCCue.openCue(c.id); await wait(800); $$('#cueList .cue-item')[0].click(); await wait(1200);
      const off = { text: $('#pvScreen').innerText.replace(/\\s+/g, ' '), bold: $$('#pvScreen b, #pvScreen strong').map(b => b.textContent).join('|') };
      const cb = $('[data-k=roleLabels]'); cb.checked = true; cb.dispatchEvent(new Event('change', { bubbles: true })); await wait(1200);
      const on = { text: $('#pvScreen').innerText.replace(/\\s+/g, ' '), saved: (await window.BC.cmd('getCue', { id: c.id })).result.items[0].roleLabels };
      return { off, on };`);
    ok(rd.off && /복 있는 사람은/.test(rd.off.text) && !/인도자|회중/.test(rd.off.text) && /악인들의/.test(rd.off.bold), '교독문 역할: 회중 줄 굵게 · 머리말 빼기', rd);
    ok(rd.on && /회중: 악인들의/.test(rd.on.text) && rd.on.saved === true, '교독문 역할: 역할 이름 보이기 (저장됨)', rd);

    // ---- 2-3. 글자 자동 숨김: 편집칸 '1 초 뒤 글자만 숨김' → 송출 → 1초 뒤 숨김 → F6 은 다시 보이며
    const ah = await ev(`const h = $('[data-k=hideAfter]'); h.value = '1'; h.dispatchEvent(new Event('input', { bubbles: true })); await wait(1500);
      const saved = (await window.BC.cmd('getCue', { id: window.BCCue.currentId() })).result.items[0].hideAfter;
      await window.BC.sendPreview(); await wait(300);
      const shown = window.BC.state.live.visible; await wait(1500);
      const hidden = window.BC.state.live.visible === false;
      await window.BC.cmd('clearAll', {}); await wait(300);
      return { saved, shown, hidden, edge: !!$('[data-k=edgeHide]') };`);
    ok(ah.saved === 1 && ah.shown === true && ah.hidden && ah.edge, '글자 자동 숨김 (N초 뒤) · 처음·끝 숨김 칸', ah);

    // ---- 2-4. 연결 점검: 도움말 → 연결 점검 → 항목들 · 오늘 큐시트 · 결과 글
    const dg = await ev(`window.BCDiag.open(); await wait(2000);
      return { rows: $$('#dgList .dg-row').length, titles: $$('#dgList .dg-title b').map(b => b.textContent), sum: ($('#dgSum') || {}).textContent, text: window.BCDiag.asText().split('\\n')[0], logsBtn: !!$('#dgLogs'), bundle: await window.BC.cmd('logBundle', {}).then(r => r.ok && /## 최근 7일 로그/.test(r.result.text)) };`);
    await shot('diag');
    await ev(`$('#modalClose').click(); await wait(300); return 1;`);
    ok(dg.rows >= 7 && dg.titles.some(t => /오늘 큐시트/.test(t)) && dg.titles.includes('카메라') && /^워십프리젠터 연결 점검/.test(dg.text || ''), '연결 점검 (항목 · 오늘 큐시트 · 카메라 · 결과 글)', dg);
    ok(dg.logsBtn && dg.bundle, '로그 묶음 (연결 점검 창 단추 · 내용)', dg);

    // ---- 2-5. 새 버전 창: 바뀐 점(마크다운) · 받기/설치 · 이번 버전 건너뛰기
    const up = await ev(`const keep = window.BC.update; window.BC.update = { current: '1.0.0', version: '9.9.9', state: 'available', mode: 'notify', notes: '## 새 기능\\n- **즉석 자막** 추가\\n- 교독문 역할', page: '' };
      window.BCUpdate.open(); await wait(300);
      const r = { title: ($('#modalTitle') || {}).textContent, items: $$('.upd-notes li').map(x => x.textContent), bold: !!$('.upd-notes li b'), open: !!$('#updOpenM'), skip: !!$('#updSkip') };
      $('#updSkip').click(); await wait(600);
      r.saved = (await window.BC.cmd('getState', {})).result.settings.skipVersion; r.closed = $('#modal').hidden;
      await window.BC.cmd('updateAction', { action: 'unskip' }); window.BC.update = keep; return r;`);
    ok(up.title === '새 버전 9.9.9' && up.items.length === 2 && up.bold && up.open && up.skip, '새 버전 창 (바뀐 점 · 받기 · 건너뛰기 단추)', up);
    ok(up.saved === '9.9.9' && up.closed, '이번 버전 건너뛰기 (저장)', up);

    // ---- 2-6. OBS 원격 조작: 창 · 켜면 연결 시도(여기엔 OBS 가 없음 → 끊김 표시) · 순서의 자동 동작에 OBS 줄
    const ob = await ev(`window.BCObs.open(); await wait(300);
      const form = { host: !!$('#obsHost'), port: ($('#obsPort') || {}).value };
      $('#obsHost').value = '127.0.0.1'; $('#obsPort').value = '4459'; $('#obsSave').click(); await wait(2500);
      const st = window.BC.state.obs; const bar = ($('#status [data-obs]') || {}).textContent || '';
      await window.BC.cmd('setObs', { enabled: false }); await wait(500);
      $('#modalClose').click(); await wait(200);
      const it = window.BCCue.selected(); $('#cueEditor [data-act=actmore]') && $('#cueEditor [data-act=actmore]').click(); await wait(300);
      return { form, enabled: st && st.enabled, connected: st && st.connected, err: st && st.error, bar, row: !!$('#cueEditor [data-a=obsScene]'), after: ($('#status [data-obs]') || null) === null };`);
    ok(ob.form.host && ob.form.port === '4455' && ob.enabled === true && ob.connected === false && /연결하지 못했습니다/.test(ob.err || '') && /OBS 원격 끊김/.test(ob.bar), 'OBS 원격 조작: 설정 · 연결 실패 안내 · 상태줄', ob);
    ok(ob.row && ob.after, 'OBS 원격 조작: 순서 자동 동작 줄 · 끄면 상태줄에서 사라짐', ob);

    // ---- 2-7. 무대 화면 고정: 순서표 → 무대 화면에 순서 목록 · 지금 화면은 아래 칸
    await ev(`const c = (await window.BC.cmd('saveCue', { cue: { name: '무대 점검', items: [{ type: 'title', title: '예배로 부름' }, { type: 'text', title: '광고', body: '식사 있습니다' }, { type: 'title', title: '설교', note: '20분' }] } })).result;
      window.BCCue.openCue(c.id); await wait(800); $$('#cueList .cue-item')[1].click(); await wait(800); await window.BC.sendPreview(); await wait(500);
      await window.BC.cmd('stagePinSet', { mode: 'order', name: '순서표' }); return 1;`);
    const sw = new BrowserWindow({ width: 1280, height: 720, show: false, webPreferences: { offscreen: true } });
    await sw.loadURL(`http://127.0.0.1:${port}/outputs/stage`);
    await wait(1500);
    const sp = await sw.webContents.executeJavaScript(`(() => ({ items: [...document.querySelectorAll('.order li')].map(l => l.className + ':' + l.querySelector('.ot').textContent), badge: document.getElementById('badge').textContent, now: document.getElementById('next').textContent }))()`);
    fs.writeFileSync(path.join(OUT, 'stage-pin.png'), (await sw.webContents.capturePage()).toPNG());
    sw.destroy();
    await ev(`await window.BC.cmd('stagePinSet', {}); await window.BC.cmd('clearAll', {}); return 1;`);
    ok(sp.items.length === 3 && /^now:광고/.test(sp.items[1]) && /^nx:설교/.test(sp.items[2]) && /고정/.test(sp.badge) && /식사/.test(sp.now), '무대 화면 고정 (순서표 · 지금 순서 · 아래 칸에 지금 화면)', sp);

    // ---- 2-8. PPT 여러 순서로 나누기 (PowerPoint 없이: loadHymn 응답을 가짜 슬라이드 6장으로)
    const ps = await ev(`const realFetch = window.fetch;
      const svg = n => 'data:image/svg+xml,' + encodeURIComponent('<svg xmlns="http://www.w3.org/2000/svg" width="160" height="90"><rect width="160" height="90" fill="#335"/><text x="70" y="55" fill="#fff" font-size="30">' + n + '</text></svg>');
      window.fetch = async (u, o) => { if (String(u).includes('/api/command') && o && /"cmd":"loadHymn"/.test(o.body)) return new Response(JSON.stringify({ ok: true, result: { mode: 'slides', slides: [1, 2, 3, 4, 5, 6].map(svg), scores: [], texts: [] } })); return realFetch(u, o); };
      const c = (await window.BC.cmd('saveCue', { cue: { name: 'PPT 점검', items: [{ type: 'hymn', title: '주보 PPT', file: 'C:\\\\가짜\\\\주보.pptx' }] } })).result;
      window.BCCue.openCue(c.id); await wait(800); $$('#cueList .cue-item')[0].click(); await wait(1000);
      const row = !!$('#cueEditor [data-k=range]');
      $('#cueEditor [data-act=pptsplit]').click(); await wait(800);
      const th = $$('.sp-th'); th[0].click(); th[3].dispatchEvent(new MouseEvent('click', { bubbles: true, shiftKey: true }));
      const r1 = $('#spRange').value; $('#spName').value = '광고'; $('#spAdd').click();
      const r2 = $('#spRange').value; $('#spRange').value = '5-6'; $('#spName').value = '설교'; $('#spAdd').click(); await wait(200);
      $('#spGo').click(); await wait(1500);
      const items = [0, 1, 2].map(k => window.BCCue.item(k)).filter(Boolean).map(x => x.title + ':' + (x.range || ''));
      $$('#cueList .cue-item')[1].click(); await wait(1200);
      const pv = window.BC.preview ? window.BC.preview.slides.length : -1;
      window.fetch = realFetch; return { row, thumbs: th.length, r1, r2, items, pv };`);
    await shot('ppt-split');
    ok(ps.row && ps.thumbs === 6 && ps.r1 === '1-4' && ps.r2 === '5', 'PPT 나누기 창 (슬라이드 · Shift 범위 · 다음 시작 번호)', ps);
    ok(ps.items && ps.items.join('|') === '광고:1-4|설교:5-6' && ps.pv === 2, 'PPT 나누기 → 순서 2개 · 범위만 송출', ps);

    // ---- 2-8b. PPT 자동으로 나누기: 규칙(AI 없이) · AI(가짜 응답) · 두 번 눌러 경계 · 모아보기 그림 · 이름 고치기
    const pa = await ev(`const realFetch = window.fetch;
      const svg = n => 'data:image/svg+xml,' + encodeURIComponent('<svg xmlns="http://www.w3.org/2000/svg" width="160" height="90"><rect width="160" height="90" fill="#353"/><text x="70" y="55" fill="#fff" font-size="30">' + n + '</text></svg>');
      const T = ['사관회', '새찬송가 8장\\n거룩 거룩 거룩', '거룩 거룩 거룩 전능하신 주님', '대표기도\\n홍길동 사관', '설교\\n선한 목자', '선한 목자는 양을 위하여', '광고', '축도\\n김철수 사관'];
      let aiArgs = null;
      window.fetch = async (u, o) => {
        if (String(u).includes('/api/command') && o && /"cmd":"loadHymn"/.test(o.body)) return new Response(JSON.stringify({ ok: true, result: { mode: 'slides', slides: T.map((_, k) => svg(k + 1)), slideTexts: T, scores: [], texts: [] } }));
        if (String(u).includes('/api/command') && o && /"cmd":"pptSplitAI"/.test(o.body)) { aiArgs = JSON.parse(o.body).args; return new Response(JSON.stringify({ ok: true, result: { model: 'test/m', parts: [{ a: 1, b: 3, name: '여는 찬송', kind: 'hymn' }, { a: 4, b: 4, name: '대표기도', kind: 'prayer' }, { a: 5, b: 8, name: '설교', kind: 'sermon' }] } })); }
        return realFetch(u, o);
      };
      const c = (await window.BC.cmd('saveCue', { cue: { name: 'PPT 자동 나누기 점검', items: [{ type: 'hymn', title: '사관회 전체', file: 'C:\\\\가짜\\\\사관회.pptx' }] } })).result;
      window.BCCue.openCue(c.id); await wait(800); $$('#cueList .cue-item')[0].click(); await wait(1000);
      const hint = !!$('#cueEditor [data-act=pptsplitai]'); // 8장이라 '길면 권하기' 는 안 나옴
      $('#cueEditor [data-act=pptsplit]').click(); await wait(800);
      $('#spRule').click(); await wait(600);
      const rule = $$('#spList li').map(li => li.querySelector('.sp-r').textContent + ':' + li.querySelector('.sp-nm').value);
      const starts = $$('.sp-th.start').length;
      // 두 번 누르기: 3번에서 새 순서 → 다시 두 번 누르면 합침
      const before = $$('#spList li').length;
      $$('.sp-th')[2].dispatchEvent(new MouseEvent('dblclick', { bubbles: true })); await wait(100);
      const split = $$('#spList li').length;
      $$('.sp-th')[2].dispatchEvent(new MouseEvent('dblclick', { bubbles: true })); await wait(100);
      const merged = $$('#spList li').length;
      // AI (그림도 보내기)
      const img = $('#spImg'); if (img) img.checked = true;
      $('#spAI').click();
      for (let k = 0; k < 40 && !/AI 로 나눴습니다/.test($('#spAiMsg').textContent); k++) await wait(250);
      const ai = $$('#spList li').map(li => li.querySelector('.sp-r').textContent + ':' + li.querySelector('.sp-nm').value);
      const sheets = aiArgs ? aiArgs.sheets.map(x => x.from + '-' + x.to + ':' + x.url.slice(0, 23)) : [];
      const sent = aiArgs ? aiArgs.texts.length : 0;
      const nm = $$('#spList .sp-nm')[0]; nm.value = '개회 찬송 (8장)'; nm.dispatchEvent(new Event('input', { bubbles: true }));
      $('#spGo').click(); await wait(1500);
      const items = [0, 1, 2, 3].map(k => window.BCCue.item(k)).filter(Boolean).map(x => x.title + ':' + (x.range || ''));
      window.fetch = realFetch; return { hint, rule, starts, before, split, merged, ai, sheets, sent, items };`);
    await shot('ppt-split-ai');
    ok(pa.rule && pa.rule.join('|') === '1:사관회|2-3:새찬송가 8장 거룩 거룩 거룩|4:대표기도 · 홍길동 사관|5-6:설교 · 선한 목자|7:광고|8:축도 · 김철수 사관' && pa.starts === 6 && !pa.hint, 'PPT 자동으로 나누기 (규칙: 표지·찬송·기도·설교·광고·축도)', pa);
    ok(pa.split === pa.before + 1 && pa.merged === pa.before, 'PPT 나누기: 두 번 눌러 새 순서 · 다시 두 번 눌러 합치기', pa);
    ok(pa.sent === 8 && pa.sheets.length === 1 && pa.sheets[0] === '1-8:data:image/jpeg;base64,' && pa.ai.join('|') === '1-3:여는 찬송|4:대표기도|5-8:설교', 'PPT AI 로 나누기 (글자 + 모아보기 그림 → 결과 목록)', pa);
    ok(pa.items && pa.items.join('|') === '개회 찬송 (8장):1-3|대표기도:4-4|설교:5-8', 'PPT AI 나누기 → 고친 이름으로 순서 3개', pa);

    // ---- 2-8c. PPT 요소 따로 고치기: 요소 꺼내기(가짜 PowerPoint 응답) → 요소를 숨긴 배경 + 그림·글상자 · 원래대로
    const pt = await ev(`const realFetch = window.fetch; const realConfirm = window.confirm; window.confirm = () => true;
      const svg = (n, c) => 'data:image/svg+xml,' + encodeURIComponent('<svg xmlns="http://www.w3.org/2000/svg" width="160" height="90"><rect width="160" height="90" fill="' + c + '"/><text x="70" y="55" fill="#fff" font-size="30">' + n + '</text></svg>');
      const S = [1, 2, 3].map(n => svg(n, '#335')), B = [1, 2, 3].map(n => svg(n, '#533'));
      let asked = null;
      window.fetch = async (u, o) => {
        if (String(u).includes('/api/command') && o && /"cmd":"loadHymn"/.test(o.body)) return new Response(JSON.stringify({ ok: true, result: { mode: 'pptx', layers: true, key: 'abc', slides: S, slideTexts: ['주일 예배', '', '광고'], scores: [], texts: [] } }));
        if (String(u).includes('/api/command') && o && /"cmd":"pptTextLayers"/.test(o.body)) { asked = JSON.parse(o.body).args; return new Response(JSON.stringify({ ok: true, result: { bgs: B, slides: [[{ id: 'p0i0', type: 'image', x: 60, y: 50, w: 30, h: 30, src: svg(9, '#e9c46a'), fit: 'contain' }, { id: 'p0t1', type: 'text', x: 5, y: 5, w: 90, h: 20, text: '주일 예배', size: 88, color: '#ffff00', bold: true, align: 'center', valign: 'middle', padding: 14, font: '맑은 고딕', fit: true, lineHeight: 1.2 }, { id: 'p0s2', type: 'shape', x: 2, y: 60, w: 20, h: 10, fill: '#1d3557', radius: 20 }, { id: 'p0i3', type: 'image', x: 30, y: 60, w: 20, h: 20, src: svg(7, '#2a9d8f'), fit: 'cover', crop: { l: 0.25, t: 0.1, r: 0, b: 0 } }], [], [{ id: 'p2t0', type: 'text', x: 10, y: 40, w: 80, h: 20, text: '광고', size: 81, color: '#ff0000', align: 'right', valign: 'middle', fit: true }]] } })); }
        return realFetch(u, o);
      };
      const c = (await window.BC.cmd('saveCue', { cue: { name: 'PPT 글자 점검', items: [{ type: 'hymn', title: '주보 PPT', file: 'C:\\\\가짜\\\\글자.pptx', edits: { 0: { items: [{ id: 'mine1', type: 'text', text: '덧붙임', x: 1, y: 80, w: 30, h: 10 }] } } }] } })).result;
      window.BCCue.openCue(c.id); await wait(800); $$('#cueList .cue-item')[0].click(); await wait(1000);
      const btn = !!$('#cueEditor [data-act=ppttexton]');
      $('#cueEditor [data-act=ppttexton]').click(); await wait(1500);
      const it = JSON.parse(JSON.stringify(window.BCCue.item(0))); // 지금 모습 (뒤에서 '원래 그림으로' 가 바꾸기 전)
      const e0 = it.edits && it.edits['0'], e1 = it.edits && it.edits['1'], e2 = it.edits && it.edits['2'];
      const pv = window.BC.preview.slides.map(s => (B.includes(s.image) ? 'bg' : S.includes(s.image) ? 'orig' : '?'));
      const txt = $$('#pvScreen .bc-etext').map(x => x.textContent);
      const imgs = $$('#pvScreen img.bc-eitem').length + $$('#pvScreen div.bc-eitem > img').length; // 그림 + 자른 그림
      const fitEl = $('#pvScreen .bc-etext[data-fit]');
      const font = fitEl ? fitEl.style.fontFamily : '';
      const row = !!$('#cueEditor [data-act=ppttextoff]');
      $('#cueEditor [data-act=ppttextoff]').click(); await wait(1200);
      const after = window.BCCue.item(0).edits || null;
      const pv2 = window.BC.preview.slides.map(s => (S.includes(s.image) ? 'orig' : '?'));
      window.fetch = realFetch; window.confirm = realConfirm;
      return { btn, asked: asked && asked.screen, e0: e0 && { pptBg: !!e0.pptBg, ids: (e0.items || []).map(x => x.id) }, e1: !!e1, e2: !!(e2 && e2.pptBg), pv, txt, imgs, font, row, after, pv2 };`);
    await shot('ppt-text');
    ok(pt.btn && pt.asked && pt.asked.w > 0 && pt.e0 && pt.e0.pptBg && pt.e0.ids.join() === 'p0i0,p0t1,p0s2,p0i3,mine1' && !pt.e1 && pt.e2 && pt.pv && pt.pv.join() === 'bg,orig,bg', 'PPT 요소 따로 고치기: 요소 꺼내기 → 요소를 숨긴 배경 + 그림·글상자 (덧붙인 것은 그대로, 쌓인 차례)', pt);
    ok(pt.txt && pt.txt.includes('주일 예배') && pt.imgs === 2 && /맑은 고딕/.test(pt.font), 'PPT 요소 따로 고치기: 미리보기에 요소 그림 · 자른 그림 · 글상자 (PPT 글꼴)', pt);
    ok(pt.row && pt.after && pt.after['0'] && pt.after['0'].items.map(x => x.id).join() === 'mine1' && !pt.after['0'].pptBg && !pt.after['2'] && pt.pv2.join() === 'orig,orig,orig', 'PPT 요소 따로 고치기: 원래대로 (덧붙인 것은 남김)', pt);

    // ---- 2-8d. 끌어서 슬라이드 차례 바꾸기: 격자(PPT 원래 슬라이드) · 되돌리기 · 목록(공지 글 덩어리) · 여러 장
    const sdr = await ev(`const realFetch = window.fetch;
      const svg = n => 'data:image/svg+xml,' + encodeURIComponent('<svg xmlns="http://www.w3.org/2000/svg" width="160" height="90"><rect width="160" height="90" fill="#353"/><text x="70" y="55" fill="#fff" font-size="30">' + n + '</text></svg>');
      window.fetch = async (u, o) => { if (String(u).includes('/api/command') && o && /"cmd":"loadHymn"/.test(o.body)) return new Response(JSON.stringify({ ok: true, result: { mode: 'slides', slides: [1, 2, 3, 4].map(svg), slideTexts: [], scores: [], texts: [] } })); return realFetch(u, o); };
      const c = (await window.BC.cmd('saveCue', { cue: { name: '끌기 점검', items: [{ type: 'hymn', title: 'PPT', file: 'C:\\\\가짜\\\\끌기.pptx' }, { type: 'text', title: '광고', body: '하나\\n---\\n둘\\n---\\n셋\\n---\\n넷' }] } })).result;
      window.BCCue.openCue(c.id); await wait(800); $$('#cueList .cue-item')[0].click(); await wait(1000);
      $('#slideView [data-sv="grid"]').click(); await wait(800);
      const pe = (type, x, y, el, extra) => el.dispatchEvent(new PointerEvent(type, { bubbles: true, clientX: x, clientY: y, button: 0, buttons: type === 'pointerup' ? 0 : 1, pointerId: 1, isPrimary: true, ...(extra || {}) }));
      const drag = async (from, to, after) => {
        to.scrollIntoView({ block: 'nearest' }); await wait(100); // 놓을 자리가 보이게 (실제로는 가장자리로 끌면 저절로 넘어감)
        const r1 = from.getBoundingClientRect(), r2 = to.getBoundingClientRect();
        pe('pointerdown', r1.left + r1.width / 2, r1.top + r1.height / 2, from);
        pe('pointermove', r1.left + r1.width / 2 + 15, r1.top + r1.height / 2 + 15, from);
        const grid = to.classList.contains('sg-cell');
        const x = grid ? (after ? r2.right - 6 : r2.left + 6) : r2.left + r2.width / 2;
        const y = grid ? r2.top + r2.height / 2 : (after ? r2.bottom - 4 : r2.top + 4);
        pe('pointermove', x, y, to);
        const mark = to.classList.contains(after ? 'drop-after' : 'drop-before') && from.classList.contains('drag-src');
        pe('pointerup', x, y, to);
        await wait(1500);
        return mark;
      };
      const cells = () => $$('#slideGrid .sg-cell');
      const m1 = await drag(cells()[0], cells()[2], true);
      const order1 = window.BCCue.item(0).slideOrder || null;
      const labels1 = window.BC.preview.slides.map(s => s.label).join(',');
      const pv1 = window.BC.preview.index;
      const grid1 = cells().map(x => x.querySelector('.sg-label').textContent.trim()).join('|');
      window.BCCue.undo(); await wait(1500);
      const undone = window.BCCue.item(0).slideOrder || null;
      // 목록: 공지의 '넷' 을 맨 위로 · Shift 로 두 장 골라 끌기
      $('#slideView [data-sv="list"]').click(); await wait(500);
      $$('#cueList .cue-item')[1].click(); await wait(1200);
      const rows = () => $$('#results .srow');
      const m2 = await drag(rows()[3], rows()[0], false);
      const body2 = window.BCCue.item(1).body;
      rows()[2].click(); await wait(300); rows()[3].dispatchEvent(new MouseEvent('click', { bubbles: true, shiftKey: true })); await wait(300);
      const m3 = await drag(rows()[3], rows()[0], false);
      const body3 = window.BCCue.item(1).body;
      window.fetch = realFetch;
      return { m1, order1, labels1, pv1, grid1, undone, m2, body2, m3, body3 };`);
    await shot('slide-drag');
    ok(sdr.m1 && JSON.stringify(sdr.order1) === '[1,2,0,3]' && sdr.labels1 === '2,3,1,4' && sdr.pv1 === 2, '끌어서 차례 바꾸기: 격자에서 PPT 1번을 3번 뒤로 (옮긴 슬라이드 고른 채로)', sdr);
    ok(sdr.undone === null, '끌어서 차례 바꾸기: Ctrl+Z 로 되돌리기', sdr);
    ok(sdr.m2 && sdr.body2 === '넷\n---\n하나\n---\n둘\n---\n셋' && sdr.m3 && sdr.body3 === '둘\n---\n셋\n---\n넷\n---\n하나', '끌어서 차례 바꾸기: 목록에서 공지 글 덩어리 · 여러 장 함께', sdr);

    // ---- 2-8e. 여기서 새 순서로 나누기 (파워포인트 '구역'): 오른쪽 클릭 · PPT(범위·꾸밈·덧붙임) · 공지 · 제목 · 되돌리기
    const sec = await ev(`const realFetch = window.fetch; const realAsk = window.BC.ask; window.BC.ask = async (q, d) => '설교';
      const svg = n => 'data:image/svg+xml,' + encodeURIComponent('<svg xmlns="http://www.w3.org/2000/svg" width="160" height="90"><rect width="160" height="90" fill="#335"/><text x="70" y="55" fill="#fff" font-size="30">' + n + '</text></svg>');
      window.fetch = async (u, o) => { if (String(u).includes('/api/command') && o && /"cmd":"loadHymn"/.test(o.body)) return new Response(JSON.stringify({ ok: true, result: { mode: 'slides', slides: [1, 2, 3, 4, 5].map(svg), slideTexts: ['표지', '찬송', '기도', '설교\\n홍길동 사관', '축도'], scores: [], texts: [] } })); return realFetch(u, o); };
      const c = (await window.BC.cmd('saveCue', { cue: { name: '구역 점검', items: [
        { type: 'hymn', title: '사관회 전체', file: 'C:\\\\가짜\\\\구역.pptx', edits: { 3: { items: [{ id: 'mine1', type: 'text', text: '메모', x: 1, y: 1, w: 10, h: 10 }] } }, extra: [{ id: 'ex1', at: 3, text: '덧붙임' }] },
        { type: 'text', title: '광고', body: '하나\\n---\\n둘\\n---\\n셋' },
        { type: 'title', title: '제목1', subtitle: '부', more: '제목2\\n---\\n제목3' }] } })).result;
      window.BCCue.openCue(c.id); await wait(800);
      $('#slideView [data-sv="list"]').click(); await wait(300);
      $$('#cueList .cue-item')[0].click(); await wait(1200);
      const why0 = window.BCSlideEdit.splitWhy(0);
      const row = $$('#results .srow')[3];
      const r = row.getBoundingClientRect();
      row.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true, clientX: r.left + 20, clientY: r.top + 10 })); await wait(300);
      const mi = $$('.ctx .ctx-item').find(x => /여기서 새 순서로 나누기/.test(x.textContent));
      const menu = !!mi && !mi.disabled;
      if (mi) mi.click();
      await wait(1600);
      const it = k => window.BCCue.item(k);
      const ppt = [0, 1].map(k => it(k) && { t: it(k).title, r: it(k).range || '', e: it(k).edits ? Object.keys(it(k).edits).join() : '', ex: it(k).extra ? it(k).extra.map(e => e.at).join() : '' });
      const pv = window.BC.preview ? window.BC.preview.slides.map(x => x.label).join(',') : '';
      $$('#cueList .cue-item')[2].click(); await wait(1000);
      await window.BCSlideEdit.splitAt(1); await wait(1300);
      const texts = [2, 3].map(k => it(k).title + ':' + it(k).body);
      $$('#cueList .cue-item')[4].click(); await wait(1000);
      await window.BCSlideEdit.splitAt(2); await wait(1300);
      const titles = [4, 5].map(k => [it(k).title, it(k).subtitle || '', it(k).more || ''].join('|'));
      const n1 = window.BCCue.cueInfo().items.length;
      window.BCCue.undo(); await wait(1300);
      const n2 = window.BCCue.cueInfo().items.length;
      window.fetch = realFetch; window.BC.ask = realAsk;
      return { why0, menu, ppt, pv, texts, titles, n1, n2 };`);
    await shot('ppt-section');
    ok(sec.why0 && sec.menu && sec.ppt && JSON.stringify(sec.ppt) === JSON.stringify([{ t: '사관회 전체', r: '1-3', e: '', ex: '' }, { t: '설교', r: '4-5', e: '0', ex: '0' }]) && sec.pv === '4,덧붙임,5', '구역 나누기: PPT 슬라이드 오른쪽 클릭 → 새 순서 (범위 · 꾸민 것 · 덧붙인 슬라이드도 함께)', sec);
    ok(sec.texts && sec.texts.join('/') === '광고:하나/설교:둘\n---\n셋' && sec.titles.join('/') === '제목1|부|제목2/제목3||' && sec.n1 === 6 && sec.n2 === 5, '구역 나누기: 공지 글 덩어리 · 제목 장 · Ctrl+Z 되돌리기', sec);

    // ---- 2-8f. 클립보드 붙여 넣기 → 새 순서 (글: 성경·찬양·공지 · 그림 · 글+그림은 묻기 · 글 입력칸은 그대로 · 편집기에서는 그 슬라이드에) · Ctrl+휠 썸네일 크기
    //   실제 클립보드는 건드리지 않는다: 붙여 넣기 이벤트를 만들어 보낸다 (PowerPoint 슬라이드는 단위 테스트에서 가짜 클립보드·가짜 PowerPoint 로)
    const pst = await ev(`
      const paste = (o, target) => { const dt = new DataTransfer(); if (o.text) dt.setData('text/plain', o.text); (o.files || []).forEach(f => dt.items.add(f)); const e = new ClipboardEvent('paste', { clipboardData: dt, bubbles: true, cancelable: true }); (target || document.body).dispatchEvent(e); return e.defaultPrevented; };
      const png = async () => { const c = document.createElement('canvas'); c.width = 160; c.height = 90; const g = c.getContext('2d'); g.fillStyle = '#2a6'; g.fillRect(0, 0, 160, 90); const b = await new Promise(r => c.toBlob(r, 'image/png')); return new File([b], 'image.png', { type: 'image/png' }); };
      const items = () => window.BCCue.cueInfo().items;
      const blur = () => { const a = document.activeElement; if (a && a.blur) a.blur(); };
      const c = (await window.BC.cmd('saveCue', { cue: { name: '붙여 넣기 점검', items: [{ type: 'title', title: '예배 시작' }] } })).result;
      window.BCCue.openCue(c.id); await wait(800); $$('#cueList .cue-item')[0].click(); await wait(500); blur();
      const p1 = paste({ text: '요 3:16' }); await wait(900);
      paste({ text: '나 같은 죄인 살리신\\n주 은혜 놀라워\\n잃었던 생명 찾았고\\n광명을 얻었네' }); await wait(900);
      paste({ text: '이번 주 일정\\n수요예배 저녁 7시 30분\\n금요기도회 저녁 9시' }); await wait(900);
      paste({ files: [await png()] }); await wait(1500);
      const made = items().map(x => [x.type, x.title, x.ref || x.body || (x.lyrics ? x.lyrics.split('\\n').length : '')].join(':'));
      const img = items()[4] && items()[4].image || '';
      const served = img ? (await fetch(img)).status : 0;
      const msg = $('#msg').textContent;
      // 글 입력칸에 붙이면 그대로 (새 순서를 만들지 않는다)
      const q = $('#q'); q.focus(); const pIn = paste({ text: '요 3:16' }, q); await wait(400); q.blur();
      const nIn = items().length;
      // 글 + 그림 (PowerPoint 도형 · 엑셀 칸): 묻는다 → '글로'
      paste({ text: '환영합니다', files: [await png()] }); await wait(600);
      const ask = $$('.ctx:not([hidden]) .ctx-item').map(x => x.textContent.trim().slice(0, 4));
      const nAsk = items().length;
      const asText = $$('.ctx:not([hidden]) .ctx-item').find(x => /^글로/.test(x.textContent.trim())); if (asText) asText.click(); await wait(900);
      const picked = items()[5] && items()[5].type + ':' + items()[5].title;
      // 실행 취소: 방금 붙인 것만 빠진다
      window.BCCue.undo(); await wait(600);
      const nUndo = items().length;
      // 슬라이드 편집기 안: 새 순서가 아니라 그 슬라이드의 글상자·그림으로
      const ti = items().findIndex(x => x.type === 'text');
      window.BCSlideEdit.open(ti, '0'); await wait(1200);
      paste({ text: '붙여 넣은 글상자' }); await wait(500); paste({ files: [await png()] }); await wait(1500);
      const ed = Object.values(items()[ti].edits || {}).flatMap(x => x.items || []).map(x => x.type + ':' + (x.text || (x.src || '').slice(0, 12)));
      const nEd = items().length;
      window.BCSlideEdit.close(); await wait(500);
      return { p1, made, served, msg, pIn, nIn, ask, nAsk, picked, nUndo, ed, nEd };`);
    await shot('paste-clipboard');
    ok(pst.p1 === true && pst.made && pst.made.join('|') === 'title:예배 시작:|bible:성경 봉독:요 3:16|song:나 같은 죄인 살리신:4|text:이번 주 일정:수요예배 저녁 7시 30분\n금요기도회 저녁 9시|image:붙여 넣은 그림:' && pst.served === 200 && /이미지 순서로/.test(pst.msg),
      '붙여 넣기: 장절 → 성경 · 가사 → 찬양 · 글 → 공지(첫 줄이 제목) · 그림 → 이미지 (고른 순서 다음에 차례로)', pst);
    ok(pst.pIn === false && pst.nIn === 5 && pst.ask && pst.ask.join() === '글로 —,그림으로' && pst.nAsk === 5 && pst.picked === 'text:환영합니다' && pst.nUndo === 5,
      '붙여 넣기: 글 입력칸은 그대로 · 글과 그림이 같이 있으면 묻기 · Ctrl+Z 되돌리기', pst);
    ok(pst.ed && pst.ed.length === 2 && pst.ed[0] === 'text:붙여 넣은 글상자' && /^image:\/media\/clip-/.test(pst.ed[1]) && pst.nEd === 5, '붙여 넣기: 슬라이드 편집기에서는 그 슬라이드의 글상자·그림으로', pst);
    const whl = await ev(`
      $('#slideView [data-sv="grid"]').click(); await wait(300);
      $$('#cueList .cue-item')[3].click(); await wait(1000);
      const g = $('#slideGrid');
      const size = () => getComputedStyle(g).getPropertyValue('--sg').trim() + '/' + $('#sgSize').value;
      const wheel = (dy, ctrl) => { const e = new WheelEvent('wheel', { deltaY: dy, ctrlKey: ctrl, bubbles: true, cancelable: true }); g.dispatchEvent(e); return e.defaultPrevented; };
      $('#sgSize').value = 240; $('#sgSize').oninput({ target: $('#sgSize') });
      const s0 = size();
      const up = wheel(-100, true); wheel(-100, true);
      const s1 = size();
      for (let k = 0; k < 12; k++) wheel(100, true);
      const s2 = size();
      const plain = wheel(100, false);
      const s3 = size();
      const saved = localStorage.getItem('wc-slide-grid-size');
      $('#sgSize').value = 240; $('#sgSize').oninput({ target: $('#sgSize') });
      $('#slideView [data-sv="list"]').click(); await wait(200);
      return { hidden: g.hidden, s0, up, s1, s2, plain, s3, saved };`);
    ok(whl.s0 === '240px/240' && whl.up === true && whl.s1 === '280px/280' && whl.s2 === '150px/150' && whl.plain === false && whl.s3 === whl.s2 && whl.saved === '150',
      'Ctrl+휠: 썸네일 크기 (위로 = 크게 · 가장 작게에서 멈춤 · 막대도 같이 · 그냥 휠은 스크롤)', whl);

    // ---- 2-9. 무대 화면 모니터: 패널에 모니터 목록 · 잘못된 모니터는 거절 (실제 모니터에 창은 띄우지 않음)
    const sm = await ev(`window.BCDock && window.BCDock.show('stage'); await wait(800);
      const sel = $('#stMon'); if (sel) { sel.dispatchEvent(new Event('focus')); await wait(600); }
      const bad = await window.BC.cmd('setStageWindow', { enabled: true, displayId: 987654 });
      return { row: !!sel && !$('#stMonRow').hidden, opts: sel ? sel.options.length : 0, bad: bad.ok ? 'ok' : bad.error, stage: window.BC.state.outputs && window.BC.state.outputs.stage };`);
    ok(sm.row && sm.opts >= 2 && /모니터를 고르세요/.test(sm.bad) && sm.stage && sm.stage.enabled === false, '무대 화면 모니터 (목록 · 잘못된 모니터 거절)', sm);

    // ---- 2-10. 한글 입력 자동 전환: 도우미가 실제로 응답 (윈도우)
    const im = await ev(`const r = await window.BC.cmd('imeKorean', {}); return { ok: r.ok, res: r.result, err: r.error, setting: window.BC.state.settings.imeAuto };`);
    ok(im.ok && im.setting !== false && (process.platform !== 'win32' || /^ok \d+ \d+$/.test(im.res && im.res.result || '')), '한글 입력 자동 전환 (도우미 응답)', im);

    // ---- 2-11. Companion: 외부 단추 창에 주소·시험 칸 · 순서 자동 동작에 Companion 줄
    const cp = await ev(`window.BCTrigger.open(); await wait(800); const r = { host: ($('#cpHost') || {}).value, test: !!$('#cpTest') }; $('#modalClose').click(); await wait(200);
      r.row = !!$('#cueEditor [data-a=companion]'); return r;`);
    ok(cp.host === '127.0.0.1' && cp.test && cp.row, 'Companion 단추 (외부 단추 창 · 순서 자동 동작 줄)', cp);

    // ---- 2-12. 장비: X32 등록 → 순서 자동 동작에 장비 줄 → 지우기
    const dv = await ev(`window.BCDevices.open(); await wait(300); $('#dvKind').value = 'x32'; $('#dvName').value = '본당 X32'; $('#dvHost').value = '127.0.0.1'; $('#dvAdd').click(); await wait(800);
      const rows = $$('.dv-row').length; $('#modalClose').click(); await wait(300);
      const it = window.BCCue.selected(); window.BCCue.openCue && 0; await wait(100);
      const row = !!$('#cueEditor [data-dev]') || !!$('#cueEditor [data-act=devopen]');
      const ds = window.BC.state.settings.devices || []; for (const d of ds) await window.BC.cmd('deleteDevice', { id: d.id });
      return { rows, row, n: ds.length };`);
    ok(dv.rows === 1 && dv.n === 1 && dv.row, '장비 (X32 등록 · 순서 자동 동작 줄)', dv);

    // ---- 3-1. 회중 메시지: Ctrl+M → 유아실 호출 틀 → 칸 채우고 Enter → 예배당 미리보기에 띠 · 상태줄 '메시지 ✕' → 숨기기
    const mg = await ev(`document.body.focus(); window.dispatchEvent(new KeyboardEvent('keydown', { key: 'm', code: 'KeyM', ctrlKey: true, bubbles: true })); await wait(500);
      const f = $('#msgFields [data-f]'); const field = f && f.dataset.f; f.value = '홍길동'; f.dispatchEvent(new Event('input', { bubbles: true }));
      const prev = $('#msgPrev').textContent; f.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true })); await wait(1200);
      const st = window.BC.state.message; const bar = ($('#status [data-msgoff]') || {}).textContent || '';
      const live = $('#lvScreen').innerText;
      $('#msgHide').click(); await wait(800);
      const after = window.BC.state.message; $('#modalClose').click(); await wait(200);
      return { field, prev, text: st && st.text, bar, live: /홍길동/.test(live), after };`);
    ok(mg.field === '이름' && /^홍길동 어린이 부모님/.test(mg.prev) && /홍길동/.test(mg.text || '') && /메시지/.test(mg.bar) && mg.after === null, '회중 메시지 (틀 칸 채우기 · 보이기 · 상태줄 · 숨기기)', mg);

    // ---- 3-2. 글 묻기(app.ask — Electron 에 prompt() 가 없음) · 매크로: 순서 동작 → 매크로로 저장 → Alt+1 · 외부 단추
    const mc = await ev(`const c = (await window.BC.cmd('saveCue', { cue: { name: '매크로 점검', items: [{ type: 'title', title: '설교', actions: { stageMsg: '설교 시작' } }] } })).result;
      window.BCCue.openCue(c.id); await wait(800); $$('#cueList .cue-item')[0].click(); await wait(800);
      if (!$('#cueEditor [data-act=macrosave]')) { $('#cueEditor [data-act=actmore]').click(); await wait(300); }
      $('#cueEditor [data-act=macrosave]').click(); await wait(300);
      const askShown = window.BCAsk.isOpen(); const def = document.querySelector('.ask-in').value;
      document.querySelector('.ask-in').value = '설교 시작'; document.querySelector('.ask-in').dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true })); await wait(1000);
      const saved = (window.BC.state.settings.macros || []).map(m => m.name);
      await window.BC.cmd('stageMessage', { text: '' }); await wait(200);
      document.body.focus(); window.dispatchEvent(new KeyboardEvent('keydown', { key: '1', code: 'Digit1', altKey: true, bubbles: true })); await wait(800);
      const afterAlt = window.BC.state.stage.message;
      await window.BC.cmd('stageMessage', { text: '' }); await wait(200);
      const info = (await window.BC.cmd('triggerInfo', {})).result; await fetch('/api/trigger/macro?name=' + encodeURIComponent('설교 시작') + '&key=' + encodeURIComponent(info.key)); await wait(1000);
      const afterTrig = window.BC.state.stage.message;
      window.BCMacro.open(); await wait(300); const cards = $$('.mc-card').length; $('#modalClose').click();
      for (const m of window.BC.state.settings.macros || []) await window.BC.cmd('deleteMacro', { id: m.id }); await window.BC.cmd('stageMessage', { text: '' });
      return { askShown, def, saved, afterAlt, afterTrig, cards };`);
    ok(mc.askShown && mc.def === '설교' && mc.saved.includes('설교 시작'), '글 묻기 창 (prompt 대신) · 매크로로 저장', mc);
    ok(mc.afterAlt === '설교 시작' && mc.afterTrig === '설교 시작' && mc.cards === 1, '매크로 실행 (Alt+1 · 외부 단추 · 매크로 창)', mc);

    // ---- 3-3. 예약 실행: 매크로 창에서 예약 더하기 → 요일 바꾸기 → 저장됨
    const sc = await ev(`await window.BC.cmd('saveMacro', { macro: { name: '예배 전', actions: { logo: 'on' } } }); await wait(400);
      window.BCMacro.open(); await wait(400); $('#scAdd').click(); await wait(800);
      const row = $('.sc-row'); row.querySelector('[data-day="3"]').click(); await wait(800);
      const s = window.BC.state.settings.schedule || []; $('#modalClose').click();
      await window.BC.cmd('saveSchedule', { list: [] }); for (const m of window.BC.state.settings.macros || []) await window.BC.cmd('deleteMacro', { id: m.id });
      return { n: s.length, days: s[0] && s[0].days, time: s[0] && s[0].time };`);
    ok(sc.n === 1 && JSON.stringify(sc.days) === '[0,3]' && sc.time === '10:55', '예약 실행 (더하기 · 요일 · 저장)', sc);

    // ---- 3-4. 큐시트 인쇄: Ctrl+P → 미리보기에 순서 · 메모 · 가사(부를 순서대로) · 성경 본문(켜면)
    const pr = await ev(`const c = (await window.BC.cmd('saveCue', { cue: { name: '인쇄 점검', serviceDate: '2026-11-01', items: [{ type: 'title', title: '예배로 부름', note: '일어서기' }, { type: 'song', title: '은혜', lyrics: '[1절]\\n은혜 아니면\\n\\n[후렴]\\n주의 은혜라', order: '1절 후렴 1절' }, { type: 'bible', title: '성경 봉독', ref: '요 3:16' }] } })).result;
      window.BCCue.openCue(c.id); await wait(900); localStorage.removeItem('wc-print');
      document.body.focus(); window.dispatchEvent(new KeyboardEvent('keydown', { key: 'p', code: 'KeyP', ctrlKey: true, bubbles: true })); await wait(1500);
      const b = () => $('#prFrame').contentDocument.body.innerText;
      const t1 = b(); const bib = $('[data-po=bible]'); if (!bib.checked) { bib.click(); await wait(1500); }
      const t2 = b(); return { open: !!$('#prFrame'), title: /인쇄 점검/.test(t1), note: /메모: 일어서기/.test(t1), order: /부를 순서: 1절 후렴 1절/.test(t1), lyr: (t1.match(/은혜 아니면/g) || []).length, bible: /하나님이 세상을/.test(t2), noBibleFirst: !/하나님이 세상을/.test(t1) };`);
    await shot('print');
    await ev(`$('#modalClose').click(); return 1;`);
    ok(pr.open && pr.title && pr.note && pr.order && pr.lyr === 2 && pr.bible && pr.noBibleFirst, '큐시트 인쇄 (메모 · 부를 순서대로 가사 · 성경 본문)', pr);

    // ---- 3-5. 부를 순서 여러 개: 라이브러리 찬양 순서에서 '지금 순서 저장' → 다른 순서로 바꿨다가 저장한 것 누르면 돌아옴
    const ar = await ev(`const s = (await window.BC.cmd('saveSong', { song: { title: '순서 점검 찬양', lyrics: '[1절]\\n가\\n\\n[후렴]\\n나' } })).result;
      const c = (await window.BC.cmd('saveCue', { cue: { name: '순서 점검', items: [{ type: 'song', title: '순서 점검 찬양', songId: s.id, lyrics: '[1절]\\n가\\n\\n[후렴]\\n나', order: '1절 후렴 후렴' }] } })).result;
      window.BCCue.openCue(c.id); await wait(900); $$('#cueList .cue-item')[0].click(); await wait(1200);
      $('#cueEditor [data-arrsave]').click(); await wait(300); document.querySelector('.ask-in').value = '1부 예배'; document.querySelector('.ask-in').dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true })); await wait(1200);
      const btn = !!$('#cueEditor [data-arr="1부 예배"]');
      const inp = $('#cueEditor [data-k=order]'); inp.value = '1절'; inp.dispatchEvent(new Event('input', { bubbles: true })); await wait(800);
      $('#cueEditor [data-arr="1부 예배"]').click(); await wait(800);
      const back = $('#cueEditor [data-k=order]').value; const lib = (await window.BC.cmd('getSong', { id: s.id })).result.arrangements;
      await window.BC.cmd('deleteSong', { id: s.id }); return { btn, back, lib };`);
    ok(ar.btn && ar.back === '1절 후렴 후렴' && ar.lib.length === 1 && ar.lib[0].name === '1부 예배', '부를 순서 여러 개 (저장 · 골라 쓰기)', ar);

    // ---- 3. 주보 순서 붙여 넣기 → 큐시트 (AI 없이)
    await ev(`await window.BC.cmd('saveSong', { song: { title: '은혜 아니면', lyrics: '은혜 아니면 나 서지 못하네' } }); return 1;`);
    const quick = await ev(`window.BCAI.open('quick'); await wait(600); $('#qkText').value = '묵도 / 찬양 은혜 아니면 / 성경봉독 요 3:16 / 설교 “선한 목자” 홍길동 목사 / 축도'; $('#qkName').value = '점검 예배'; $('#qkGo').click(); await wait(2500);
      return { done: ($('#aiResult .ai-done b') || {}).textContent, items: $$('#aiResult li').map(x => x.innerText.replace(/\\s+/g, ' ')) };`);
    ok(/점검 예배.*5개/.test(quick.done || ''), '주보 순서 붙여 넣기 → 큐시트', quick);
    ok(quick.items && quick.items[1] === '은혜 아니면' && /요 3:16/.test(quick.items[2] || ''), '순서 알아보기 (찬양·장절)', quick);
    await shot('quick');
    await ev(`$('#aiOpen').click(); await wait(1000); return 1;`);

    // ---- 3-1. 설교 포인트 슬라이드: 열린 큐시트의 설교·성경 순서로 미리 채우고 → 대지 찾기 → '설교' 다음에 넣기
    const pts = await ev(`window.BCPoints.open(); await wait(500);
      const pre = { title: $('#ptTitle').value, verse: $('#ptVerse').value, speaker: $('#ptSpeaker').value };
      $('#ptText').value = '첫째, 선한 목자는 양을 아십니다.\\n주님은 우리 이름을 부르십니다.\\n둘째, 선한 목자는 목숨을 버립니다.\\n십자가가 그 증거입니다.';
      $('#ptFind').click(); await wait(1000);
      const cards = $$('#ptCards .pts-card').map(c => c.innerText.replace(/\\s+/g, ' '));
      $('#ptInsert').click(); await wait(1200);
      const names = []; for (let i = 0; i < 30; i++) { const it = window.BCCue.item(i); if (!it) break; names.push((it.note || '').startsWith('설교 포인트') ? 'PTS:' + it.title : it.title); }
      return { pre, cards, names, modal: !$('#modal').hidden };`);
    ok(pts.pre && pts.pre.title === '선한 목자' && /요 3:16/.test(pts.pre.verse) && /홍길동/.test(pts.pre.speaker), '설교 포인트: 설교·성경 순서로 미리 채움', pts);
    ok(pts.cards && pts.cards.length === 3 && /첫째, 선한 목자는 양을 아십니다/.test(pts.cards[1]), '설교 포인트: 대지 찾기 → 카드 미리보기', pts);
    ok(pts.names && pts.names.indexOf('PTS:선한 목자') === pts.names.indexOf('설교') + 1 && pts.modal === false, '설교 포인트: 설교 다음에 큐시트로', pts);

    // ---- 4. 베들레헴 찬송가 (있을 때만)
    const books = await ev(`return (await window.BC.cmd('hymnalBooks', {})).result;`);
    if (Array.isArray(books) && books.some(b => b.lyrics)) {
      const h = await ev(`const r = await window.BC.cmd('hymnalGet', { book: '새찬송가', number: 1 }); return r.ok ? { kind: r.result.kind, title: r.result.title, lyrics: (r.result.lyrics || '').slice(0, 20) } : { err: r.error };`);
      ok(h.kind === 'hymn' && /^1장 /.test(h.title || ''), '베들레헴 찬송가 가사', h);
    } else R.log.push('– 베들레헴 찬송가 없음 (건너뜀)');

    // ---- 5. 카메라: 격자 썸네일은 📷 표시만, 미리보기는 잠시 뒤 정지 화면 → 카메라 놓기
    await ev(`window.WC_CAM_PREVIEW_MS = 3000; return 1;`);
    const dev = await ev(`const s = await navigator.mediaDevices.getUserMedia({ video: true }); s.getTracks().forEach(t => t.stop()); return (await navigator.mediaDevices.enumerateDevices()).filter(d => d.kind === 'videoinput').map(d => d.deviceId);`);
    if (Array.isArray(dev) && dev.length) {
      const cam = await ev(`const c = (await window.BC.cmd('saveCue', { cue: { name: '카메라 점검', items: [{ type: 'camera', deviceId: ${JSON.stringify(dev[0])}, label: '가짜', title: '📷 가짜' }, { type: 'title', title: '다음' }] } })).result;
        window.BCCue.openCue(c.id); await wait(800); $$('#cueList .cue-item')[0].click(); await wait(2000);
        const live = { video: !!$('#pvScreen video[data-cam]'), grid: $$('.sg-screen video[data-cam]').length, still: $$('.sg-screen .bc-cam-still').length, open: window.BCRender.openCameras().length };
        await wait(8000);
        const later = { frozen: !!$('#pvScreen .bc-cam-frozen'), open: window.BCRender.openCameras().length, holders: $$('video[data-cam]').length };
        $$('#cueList .cue-item')[1].click(); await wait(500);
        return { live, later };`);
      ok(cam.live && cam.live.video && cam.live.open === 1, '카메라 미리보기 (산 영상)', cam);
      ok(cam.live && cam.live.grid === 0, '격자 썸네일은 산 영상이 아님', cam);
      ok(cam.later && cam.later.frozen && cam.later.open === 0 && cam.later.holders === 0, '미리보기 정지 화면 → 카메라 놓기 (붙든 곳 없음)', cam);
    } else R.log.push('– 가짜 카메라 없음 (건너뜀)');

    // ---- 6. 도킹(OBS·리모컨) '■ 끄기'
    const before = await ev(`$('#q').focus(); $('#q').value = '시 23:1'; $('#q').dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true })); await wait(1500); await window.BC.sendPreview(); await wait(800); return window.BC.state.live.visible;`);
    const d = new BrowserWindow({ width: 420, height: 900, show: false, webPreferences: { offscreen: true } });
    await d.loadURL(`http://127.0.0.1:${port}/dock`);
    await wait(2500);
    const off = await d.webContents.executeJavaScript(`(() => { const b = document.getElementById('dOff'); if (!b) return false; b.click(); return true; })()`);
    await wait(1500);
    const offState = await ev(`return window.BC.state.live.visible;`);
    ok(before === true && off && offState === false, "도킹 '■ 끄기'", { before, off, offState });
    d.destroy();

    // ---- 7. 영어 화면: UI 는 영어, 사용자 글은 그대로
    await ev(`await window.BC.cmd('saveCue', { cue: { name: '찬송', items: [{ type: 'title', title: '설교', subtitle: '축도' }] } }); await window.BC.cmd('updateSettings', { patch: { uiLang: 'en' } }); return 1;`);
    await wait(400);
    await load();
    const en = await ev(`const o = $$('#cueSel option').find(o => /찬송/.test(o.textContent)); if (o) { $('#cueSel').value = o.value; $('#cueSel').dispatchEvent(new Event('change', { bubbles: true })); } await wait(800);
      return { lang: document.documentElement.dataset.lang, menu: $$('.menubar button, #menubar button').slice(0, 3).map(b => b.textContent.trim()), cueSel: o && o.textContent, row: ($('#cueList .cue-item') || {}).innerText };`);
    ok(en.lang === 'en' && /File|Cue/.test((en.menu || []).join(' ')), '영어 화면 (UI 번역)', en);
    ok(/찬송/.test(en.cueSel || '') && /설교/.test(en.row || '') && /축도/.test(en.row || ''), '영어 화면에서 사용자 글은 그대로', en);
    await shot('en');
    await ev(`await window.BC.cmd('updateSettings', { patch: { uiLang: 'ko' } }); return 1;`);

    // ---- 8. 사용 기록 창 · 설정의 동기화·백업 · 큐 편집 실행 취소
    const usage = await ev(`if (!$('#modal').hidden) $('#modalClose').click(); await wait(300); window.BCUsage.open(); await wait(1000); const r = { list: !!$('#ugList'), rows: (await window.BC.cmd('usageLog', { kinds: ['bible'] })).result.rows.length }; $('#modalClose').click(); await wait(300); return r;`);
    ok(usage.list && usage.rows >= 1, '사용 기록 창 (송출한 본문이 보임)', usage);
    const setData = await ev(`window.BCSettings.open('data'); await wait(1000); const r = { sync: !!$('#syDir'), backup: !!$('#bkNow'), info: ($('#syInfo') || {}).textContent }; $('#modalClose').click(); await wait(300); return r;`);
    ok(setData.sync && setData.backup && /동기화 안 함/.test(setData.info || ''), '설정 → 동기화·백업 칸', setData);
    const undo = await ev(`const c = (await window.BC.cmd('saveCue', { cue: { name: '되돌리기', items: [{ type: 'title', title: '하나' }] } })).result;
      window.BCCue.openCue(c.id); await wait(800); window.BCCue.addItem('title', { title: '둘' }); await wait(500);
      const n1 = $$('#cueList .cue-item').length; document.body.focus(); window.dispatchEvent(new KeyboardEvent('keydown', { key: 'z', ctrlKey: true, bubbles: true })); await wait(600);
      const n2 = $$('#cueList .cue-item').length; window.dispatchEvent(new KeyboardEvent('keydown', { key: 'y', ctrlKey: true, bubbles: true })); await wait(600);
      return { n1, n2, n3: $$('#cueList .cue-item').length };`);
    ok(undo.n1 === 2 && undo.n2 === 1 && undo.n3 === 2, '큐 편집 실행 취소 (Ctrl+Z) · 다시 실행 (Ctrl+Y)', undo);
    ok(R.console.length === 0, '화면 오류(console) 없음', R.console);
  } catch (e) { R.fails.push('점검 중 오류: ' + String(e.stack).slice(0, 500)); }
  fs.writeFileSync(path.join(OUT, 'smoke.json'), JSON.stringify(R, null, 1));
  app.exit(0);
});
