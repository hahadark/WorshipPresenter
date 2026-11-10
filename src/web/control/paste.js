/* 클립보드 붙여 넣기 → 큐시트의 새 순서 (고른 순서 다음에)
 * - Ctrl+V (글 입력칸 밖에서) · 큐시트 메뉴 · 큐시트 오른쪽 클릭 = 알아서, Ctrl+Shift+V = 종류를 골라서
 * - 탐색기에서 복사한 파일 → 끌어다 놓은 것과 같게 (PPT → 찬송가·PPT, 그림 → 이미지, 영상, 음악, txt → 찬양)
 * - PowerPoint 에서 복사한 슬라이드 → 찬송가·PPT 순서 (코어 clippaste.js 가 PPT 파일로 만든다 — 윈도우 데스크톱 프로그램)
 * - 그림 (화면 캡처 · 복사한 그림) → 이미지 순서 (/api/media 에 올린다)
 * - 글 → 장절이면 성경, 가사 모양이면 찬양, 그 밖은 공지 (코어 pasteTextPlan)
 * - 글과 그림이 같이 있으면 (PowerPoint 도형 · 엑셀 칸) 어느 쪽으로 넣을지 묻는다
 * - 슬라이드 편집기가 열려 있으면 새 순서 대신 그 슬라이드에 그림·글상자로
 * 메뉴에서 시킨 붙여 넣기는 코어 pasteNow 가 조작 화면에 '붙여 넣기'를 시켜 같은 paste 이벤트로 들어온다 (파일 경로까지 받으려고)
 */
(function () {
  'use strict';
  const $ = s => document.querySelector(s);
  const app = window.BC;
  const cmd = (...a) => app.cmd(...a);
  let force = ''; // 메뉴에서 시킨 붙여 넣기 'auto' | 'pick' — 글 입력칸에 초점이 있어도 새 순서로
  let forceTimer = 0;
  let busy = false;

  const editable = el => !!(el && el.closest && el.closest('input, textarea, select, [contenteditable=""], [contenteditable="true"]'));
  const blocked = () => !$('#modal').hidden || (window.BCAsk && window.BCAsk.isOpen()); // 창이 열려 있으면 그 창의 것
  const clearForce = () => { force = ''; clearTimeout(forceTimer); };
  const short = (s, n = 24) => { s = String(s || '').replace(/\s+/g, ' ').trim(); return s.length > n ? s.slice(0, n - 1) + '…' : s; };
  const TYPE_NAME = { bible: '성경', song: '찬양', text: '공지', title: '제목' };
  const T = s => (window.BCI18n ? window.BCI18n.t(s) : s); // 새 순서의 기본 제목은 화면 언어로 (제목은 사용자 글이라 나중에 번역되지 않는다)

  function onPaste(e) {
    const forced = force;
    if (!forced && (editable(e.target) || editable(document.activeElement))) return; // 글 입력칸: 원래 붙여 넣기
    clearForce();
    if (blocked()) return;
    const cd = e.clipboardData;
    if (!cd) return;
    // 클립보드 내용은 이 이벤트 안에서만 읽힌다 — 먼저 다 꺼내 둔다
    const data = { files: [...(cd.files || [])], text: cd.getData('text/plain') || '', html: cd.getData('text/html') || '' };
    e.preventDefault();
    handle(data, { pick: forced === 'pick' });
  }

  async function handle(data, opt = {}) {
    if (busy) return;
    busy = true;
    try { await paste(data, opt); } catch (err) { app.showMsg(err.message, 'error', 5000); } finally { busy = false; }
  }

  /** 그림 올리기 → 주소 */
  async function upload(file) {
    const ext = { 'image/png': '.png', 'image/jpeg': '.jpg', 'image/gif': '.gif', 'image/webp': '.webp', 'image/svg+xml': '.svg' }[file.type] || '.png';
    const r = await fetch('/api/media', { method: 'POST', headers: { 'X-WorshipCue': '1', 'X-Filename': 'clip' + ext, 'X-Kind': 'clip', 'Content-Type': 'application/octet-stream' }, body: file }).then(x => x.json()).catch(err => ({ ok: false, error: err.message }));
    if (!r.ok) throw new Error(r.error || '그림을 저장하지 못했습니다.');
    return r.url;
  }
  /** 글만 있는 HTML → 글 (text/plain 이 없을 때) */
  function htmlText(html) {
    try { return new DOMParser().parseFromString(html, 'text/html').body.innerText || ''; } catch { return ''; }
  }
  /** 새 순서를 넣은 뒤: 제목 칸에 간 초점을 거둔다 — 이어서 Ctrl+V 를 눌러도 제목 칸이 아니라 새 순서로 들어가게 */
  function afterAdd() {
    setTimeout(() => { const a = document.activeElement; if (a && a.matches && a.matches('#cueEditor [data-k="title"]')) a.blur(); }, 80);
  }

  async function addImages(blobs) {
    const Cue = window.BCCue;
    let n = 0;
    for (const f of blobs.slice(0, 20)) {
      const url = await upload(f);
      Cue.addItem('image', { title: blobs.length > 1 ? `${T('붙여 넣은 그림')} ${++n}` : T('붙여 넣은 그림'), image: url });
    }
    afterAdd();
    app.showMsg(blobs.length > 1 ? `그림 ${blobs.length}장을 이미지 순서로 붙여 넣었습니다.` : '그림을 이미지 순서로 붙여 넣었습니다.', 'ok', 2500);
  }
  async function addText(text, as = '') {
    const r = await cmd('pasteTextPlan', { text, as });
    if (!r.ok) return app.showMsg(r.error, 'info', 2500);
    const { kind, items } = r.result;
    for (const it of items) window.BCCue.addItem(it.type, it.type === 'bible' ? { ...it.extra, title: T(it.extra.title) } : it.extra);
    afterAdd();
    const what = kind === 'bible' ? items.map(it => it.extra.ref).join(' · ') : short(items[0].extra.title || text);
    app.showMsg(`${TYPE_NAME[kind]} 순서로 붙여 넣었습니다: ${what}${as ? '' : ' — 다른 종류로 넣으려면 Ctrl+Shift+V'}`, 'ok', 3500);
  }
  async function addSlides() {
    app.showMsg('PowerPoint 슬라이드를 가져오는 중…', 'info', 60000);
    const r = await cmd('pasteSlides', {});
    if (!r.ok) { app.showMsg(r.error, 'error', 6000); return false; }
    const s = r.result;
    window.BCCue.addItem('hymn', { title: s.title || T('붙여 넣은 슬라이드'), file: s.file, number: null, background: 'default' });
    afterAdd();
    app.showMsg(`슬라이드 ${s.count}장을 찬송가·PPT 순서로 붙여 넣었습니다.${s.how === 'plain' ? ' 원래 테마가 빠져 모양이 다르면 PPT 파일을 큐시트에 끌어다 놓으세요.' : ''}`, 'ok', s.how === 'plain' ? 7000 : 3000);
    return true;
  }

  /** 어느 것으로 넣을지 고르는 작은 메뉴 (큐시트 위에) */
  function choose(head, list) {
    if (!window.BCCtx) return;
    const r = ($('#cueList') || document.body).getBoundingClientRect();
    window.BCCtx.show(Math.max(8, r.left + 24), Math.max(8, r.top + 40), [{ head }, ...list]);
  }

  async function paste({ files, text, html }, { pick = false } = {}) {
    const Cue = window.BCCue, SE = window.BCSlideEdit;
    const pathOf = window.wcDesktop && window.wcDesktop.pathOf;
    const paths = [], blobs = [];
    for (const f of files) {
      const p = pathOf ? pathOf(f) : '';
      if (p) paths.push(p); // 탐색기에서 복사한 파일
      else if (/^image\//.test(f.type)) blobs.push(f); // 클립보드 그림
    }
    if (!text.trim() && html && !blobs.length && !/<img\b/i.test(html)) text = htmlText(html);
    const hasText = !!text.trim();

    // 슬라이드 편집기: 그 슬라이드에 그림·글상자로
    if (SE && SE.isOpen()) {
      if (paths.length) return window.BCDrop.addPaths(paths, document.body);
      if (blobs.length && !hasText) { for (const f of blobs.slice(0, 10)) SE.dropImage(await upload(f), -1, -1); return; }
      if (hasText) { SE.pasteText(text); return; }
      return app.showMsg('클립보드에 붙여 넣을 글·그림이 없습니다.', 'info', 2000);
    }
    if (!Cue || !Cue.cueInfo()) return app.showMsg('큐시트를 먼저 열거나 만드세요.', 'info', 2500);
    if (paths.length) return window.BCDrop.addPaths(paths, document.body);

    const info = await cmd('clipboardInfo', {});
    const ci = info.ok ? info.result : {};
    if (pick) {
      const plan = hasText ? await cmd('pasteTextPlan', { text }) : null;
      const kind = plan && plan.ok ? plan.result.kind : '';
      const mark = k => (k === kind ? ' (알아서 고른 것)' : '');
      const list = [];
      if (hasText) {
        if (kind === 'bible') list.push({ ic: 'book-open', label: '성경 — 장절로 본문 찾기' + mark('bible'), run: () => handle2(() => addText(text, 'bible')) });
        list.push({ ic: 'megaphone', label: '공지 — 글 슬라이드' + mark('text'), run: () => handle2(() => addText(text, 'text')) });
        list.push({ ic: 'music', label: '찬양 — 가사 슬라이드' + mark('song'), run: () => handle2(() => addText(text, 'song')) });
        list.push({ ic: 'tag', label: '제목 — 제목 카드', run: () => handle2(() => addText(text, 'title')) });
      }
      if (blobs.length) list.push({ ic: 'image', label: blobs.length > 1 ? `이미지 — 그림 ${blobs.length}장` : '이미지 — 그림 한 장', run: () => handle2(() => addImages(blobs)) });
      if (ci.slides || (ci.desktop && ci.win && blobs.length)) list.push({ ic: 'music-2', label: 'PowerPoint 슬라이드 — 찬송가·PPT' + (ci.slides ? ' (알아서 고른 것)' : ''), run: () => handle2(addSlides) });
      if (!list.length) return app.showMsg('클립보드에 붙여 넣을 글·그림·슬라이드가 없습니다.', 'info', 2500);
      return choose('어떤 순서로 붙여 넣을까요?', list);
    }
    // PowerPoint 에서 복사한 슬라이드. 못 가져오면 클립보드의 그림(첫 장)이라도
    if (ci.slides) {
      if (await addSlides()) return;
      if (blobs.length) return addImages(blobs);
      return;
    }
    if (blobs.length && hasText) { // PowerPoint 도형 · 엑셀 칸: 글과 그림이 같이 온다
      return choose('글과 그림이 같이 있습니다', [
        { ic: 'megaphone', label: `글로 — ${short(text, 28)}`, run: () => handle2(() => addText(text)) },
        { ic: 'image', label: '그림으로 — 보이는 모양 그대로', run: () => handle2(() => addImages(blobs)) },
      ]);
    }
    if (blobs.length) return addImages(blobs);
    if (hasText) return addText(text);
    app.showMsg(/<img\b/i.test(html) ? '이 그림은 클립보드에 주소만 있어 붙여 넣을 수 없습니다. 그림을 저장해 큐시트에 끌어다 놓으세요.' : '클립보드에 붙여 넣을 글·그림·슬라이드가 없습니다.', 'info', 3500);
  }
  /** 고르는 메뉴에서 누른 것 (오류는 알림으로) */
  function handle2(fn) {
    if (busy) return;
    busy = true;
    Promise.resolve().then(fn).catch(err => app.showMsg(err.message, 'error', 5000)).finally(() => { busy = false; });
  }

  /** 메뉴·오른쪽 클릭·Ctrl+Shift+V: mode 'auto' (알아서) | 'pick' (골라서) */
  async function run(mode = 'auto') {
    if (blocked()) return;
    const info = await cmd('clipboardInfo', {});
    if (info.ok && info.result.canPaste) {
      // 데스크톱 프로그램: 조작 화면에 붙여 넣기를 시키면 paste 이벤트로 들어온다
      const a = document.activeElement;
      if (a && a.blur && editable(a)) a.blur();
      force = mode === 'pick' ? 'pick' : 'auto';
      clearTimeout(forceTimer);
      forceTimer = setTimeout(() => { if (force) { force = ''; app.showMsg('큐시트를 누른 뒤 Ctrl+V 를 눌러 붙여 넣으세요.', 'info', 3000); } }, 1500);
      const r = await cmd('pasteNow', {});
      if (!r.ok) { clearForce(); app.showMsg(r.error, 'error', 4000); }
      return;
    }
    // 브라우저로 연 조작 화면: 클립보드 읽기를 허락하면 읽고, 아니면 Ctrl+V 로
    try {
      const items = await navigator.clipboard.read();
      const files = [];
      let text = '', html = '';
      for (const it of items) {
        for (const t of it.types) {
          const b = await it.getType(t);
          if (/^image\//.test(t)) files.push(new File([b], 'clip', { type: t }));
          else if (t === 'text/plain') text = await b.text();
          else if (t === 'text/html') html = await b.text();
        }
      }
      return handle({ files, text, html }, { pick: mode === 'pick' });
    } catch {
      app.showMsg('큐시트를 누른 뒤 Ctrl+V 를 눌러 붙여 넣으세요.', 'info', 3000);
    }
  }

  function init() {
    document.addEventListener('paste', onPaste);
    // Ctrl+Shift+V (글 입력칸 밖): 골라 붙여 넣기. 브라우저의 '서식 없이 붙여 넣기'는 글만 주기 때문에 따로 읽는다
    window.addEventListener('keydown', e => {
      if (!(e.ctrlKey || e.metaKey) || !e.shiftKey || e.altKey || e.code !== 'KeyV' || e.repeat) return;
      if (editable(e.target) || blocked() || (app.capturing)) return;
      e.preventDefault();
      run('pick');
    }, true);
  }

  window.BCPaste = { init, run, handle };
})();
