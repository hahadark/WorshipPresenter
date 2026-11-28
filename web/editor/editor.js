/* 웹 편집기 (crates/wp-core/src/webedit.rs)
 * - PIN (정하지 않았으면 이 PC 에서만 PIN 없이). 읽기는 X-PIN, 쓰기는 X-PIN + X-WorshipCue.
 * - 불러올 때의 updatedAt 을 base 로 보내고, 그 사이 다른 곳이 고쳤으면 서버가 409 → 덮어쓰지 않고 묻는다.
 * - 10초마다 목록을 다시 받아, 열린 큐시트가 다른 곳에서 바뀌었으면 (고친 것이 없으면) 저절로 다시 불러오고 (있으면) 미리 알린다.
 * 시험(tools/webedit/e2e.js)이 보는 것: #login #pin #go #app #list button[data-id] .item input[data-f=title] [data-add=bible] input[data-f=ref] #save #msg('저장')
 */
(function () {
  'use strict';
  var $ = function (id) { return document.getElementById(id); };
  var W = window.WPW, t = W.t, esc = W.esc;

  var TYPES = {
    title: t('제목', 'Title'), song: t('찬양', 'Song'), bible: t('성경', 'Bible'), text: t('공지', 'Announcement'), blank: t('빈 화면', 'Blank'),
    hymn: t('찬송가·PPT', 'Hymn·PPT'), image: t('그림', 'Image'), video: t('영상', 'Video'), camera: t('카메라', 'Camera'), slideshow: t('슬라이드쇼', 'Slideshow'), audio: t('음악', 'Audio'),
    weblyrics: t('웹 가사', 'Web lyrics'), ppt: 'PPT'
  };
  // 종류별로 고칠 칸: [열쇠, 이름, 여러 줄, 자리 표시]
  var FIELDS = {
    title: [['subtitle', t('부제', 'Subtitle'), 0, t('예: 시편 100편', 'e.g. Psalm 100')]],
    song: [['lyrics', t('가사', 'Lyrics'), 1, t('빈 줄 = 다음 장 · [1절] [후렴] 처럼 이름표', 'Blank line = next slide · labels like [Verse 1] [Chorus]')], ['order', t('부를 순서', 'Arrangement'), 0, t('예: 1절 후렴 2절 후렴', 'e.g. Verse1 Chorus Verse2 Chorus')]],
    bible: [['ref', t('장절', 'Reference'), 0, t('예: 요 3:16-18', 'e.g. John 3:16-18')]],
    text: [['body', t('글', 'Text'), 1, t('--- 줄 = 다음 장', 'A line with --- = next slide')]]
  };
  var NEW = ['title', 'song', 'bible', 'text', 'blank']; // 서버 webedit::NEW_TYPES 와 같음

  var pin = W.session('wp-edit-pin');
  var cues = [], active = '';
  var cur = null;          // 열린 큐시트 { id, name, serviceDate, memo, items, updatedAt }
  var orig = '';           // 불러온 그대로 (고쳤는지 보려고)
  var origItems = {};      // id → 불러온 순서 JSON (바뀐 줄 표시)
  var dirty = false, saving = false, lastSaved = 0, staleRemote = false;
  var sel = -1, open = -1, confirmDel = -1;
  var undo = [];
  var lockUntil = 0;

  // ---------- 서버 ----------
  function api(path, body) {
    var h = { 'X-PIN': pin };
    var opt = body ? { method: 'POST', headers: Object.assign(h, { 'Content-Type': 'application/json', 'X-WorshipCue': '1' }), body: JSON.stringify(body) } : { headers: h };
    return W.request(path, opt, body ? 15000 : 8000).then(function (j) {
      if (j.status === 401 || j.status === 429 || (j.status === 403 && j.error)) {
        if (j.status === 429) lockUntil = Date.now() + 30000;
        W.session('wp-edit-pin', null);
        showLogin(j.status === 429 ? 'lock' : j.status === 403 ? 'lan' : pin ? 'wrong' : '');
        j.auth = true;
      }
      return j;
    });
  }

  // ---------- PIN ----------
  function showLogin(why) {
    $('app').hidden = true;
    $('login').hidden = false;
    $('go').disabled = false;
    $('go').classList.remove('busy');
    $('go').textContent = t('열기', 'Open');
    var m = why === 'wrong' ? [t('PIN 이 맞지 않습니다', 'Wrong PIN'), t('조작 PC 의 연결 → 웹 편집기 창에 적힌 PIN 을 다시 확인하세요. 5번 틀리면 30초 동안 막힙니다.', 'Check the PIN in Connect → Web editor on the control PC. Five wrong tries lock it for 30 seconds.')]
      : why === 'lock' ? [t('PIN 을 여러 번 틀렸습니다', 'Too many wrong PINs'), t('30초 뒤에 다시 넣어 주세요.', 'Try again in 30 seconds.')]
      : why === 'lan' ? [t('이 컴퓨터에서는 아직 열 수 없습니다', 'Not available from this computer yet'), t('조작 PC 의 연결 → 웹 편집기에서 PIN 을 정하면 다른 컴퓨터에서도 열립니다.', 'Set a PIN in Connect → Web editor on the control PC to allow other computers.')]
      : why === 'net' ? [t('PC 에 닿지 않습니다', 'Cannot reach the PC'), t('이 컴퓨터가 PC 와 같은 네트워크인지, PC 에서 워십프리젠터와 웹 편집기가 켜져 있는지 확인하세요.', 'Check that this computer is on the same network and that Worship Presenter and the web editor are on.')]
      : null;
    $('lerr').className = 'msg' + (m ? ' err' : '');
    $('lerr').innerHTML = m ? '<b>' + esc(m[0]) + '</b><span>' + esc(m[1]) + '</span>' : '';
    $('pin').classList.toggle('bad', !!m && why !== 'net');
    if (lockUntil > Date.now()) lockTick();
    setTimeout(function () { try { $('pin').focus(); $('pin').select(); } catch (_) { /* 무시 */ } }, 30);
  }
  function lockTick() {
    var left = Math.ceil((lockUntil - Date.now()) / 1000);
    if (left > 0 && !$('login').hidden) { $('go').disabled = true; $('go').textContent = t('잠시 기다려 주세요 · ', 'Please wait · ') + left + t('초', 's'); setTimeout(lockTick, 500); }
    else { $('go').disabled = false; $('go').textContent = t('열기', 'Open'); }
  }
  $('go').onclick = function () {
    var v = $('pin').value.trim();
    if (!v) { $('lerr').className = 'msg err'; $('lerr').innerHTML = '<b>' + t('PIN 을 넣어 주세요.', 'Enter the PIN.') + '</b>'; return; }
    pin = v;
    $('go').disabled = true;
    $('go').classList.add('busy');
    $('go').textContent = t('여는 중…', 'Opening…');
    loadList(true);
  };
  $('pin').addEventListener('keydown', function (e) { if (e.key === 'Enter') $('go').click(); });

  // ---------- 목록 ----------
  function loadList(fromLogin) {
    return api('/e/state').then(function (j) {
      if (j.auth) return;
      if (!j.ok) {
        if (fromLogin || !$('login').hidden || $('app').hidden) { showLogin(j.network ? 'net' : ''); if (!j.network) { $('lerr').className = 'msg err'; $('lerr').innerHTML = '<b>' + esc(j.error || t('열지 못했습니다', 'Could not open')) + '</b>'; } }
        else setState('err', t('PC 와 끊김', 'Offline'));
        return;
      }
      if (pin) W.session('wp-edit-pin', pin);
      $('login').hidden = true;
      $('app').hidden = false;
      cues = j.cues || [];
      active = j.active || '';
      drawList();
      // 열린 큐시트가 다른 곳에서 바뀌었나
      if (cur) {
        var me = cues.filter(function (c) { return c.id === cur.id; })[0];
        if (me && me.updatedAt && me.updatedAt !== cur.updatedAt && !saving) {
          if (!dirty) { openCue(cur.id, true).then(function () { W.toast(t('다른 곳에서 고친 내용을 불러왔습니다', 'Loaded changes made elsewhere'), cur ? cur.name : '', { icon: 'refresh-cw', key: 'remote' }); }); }
          else if (!staleRemote) { staleRemote = true; banner('info', t('다른 곳에서 이 큐시트를 고쳤습니다', 'This cue sheet was changed elsewhere'), t('지금 저장하면 그쪽 고침과 부딪칩니다. 내 고침이 적으면 [다시 불러오기] 뒤에 다시 고치세요.', 'Saving now will conflict. If your changes are small, reload and redo them.'), [[t('다시 불러오기', 'Reload'), reloadDiscard, 'send']]); }
        }
      }
      if (!cur && !$('app').classList.contains('editing')) {
        // 처음: 조작 화면에 열린 큐시트를 먼저 보여 준다 (넓은 화면에서만 — 휴대폰은 목록부터)
        if (active && window.innerWidth >= 760 && !loadList.first) { loadList.first = true; openCue(active); }
      }
    });
  }
  function drawList() {
    var h = cues.map(function (c) {
      var meta = [c.serviceDate || '', t('순서 ', '') + c.count + t('개', c.count === 1 ? ' item' : ' items')].filter(Boolean).join(' · ');
      return '<button data-id="' + esc(c.id) + '" class="' + (cur && cur.id === c.id ? 'on' : '') + '" aria-current="' + !!(cur && cur.id === c.id) + '">' +
        '<span class="nm">' + esc(c.name) + (c.id === active ? '<span class="tag">' + t('조작 화면에 열림', 'Open on PC') + '</span>' : '') + (c.template ? '<span class="tag">' + t('틀', 'Template') + '</span>' : '') + '</span>' +
        '<small>' + esc(meta) + '</small></button>';
    }).join('');
    $('list').innerHTML = h || '<div class="none">' + t('큐시트가 없습니다. 위 [새 큐시트] 로 만드세요.', 'No cue sheets yet. Make one with New.') + '</div>';
  }
  $('list').onclick = function (e) {
    var b = e.target.closest('button[data-id]');
    if (!b) return;
    // 이미 열린 큐시트: 다시 불러오지 않는다 (그 사이 고친 것을 덮지 않게). 휴대폰은 표 화면으로만.
    if (cur && cur.id === b.dataset.id) { $('app').classList.add('editing'); return; }
    openCue(b.dataset.id);
  };
  $('newBtn').onclick = function () { $('newRow').hidden = !$('newRow').hidden; if (!$('newRow').hidden) $('newname').focus(); };
  $('newRow').onsubmit = function (e) {
    e.preventDefault();
    if (dirty && !confirm(t('저장하지 않은 고침이 있습니다. 버리고 새 큐시트를 만들까요?', 'You have unsaved changes. Discard them and make a new cue sheet?'))) return;
    var b = $('new');
    b.classList.add('busy');
    api('/e/new', { name: $('newname').value }).then(function (j) {
      b.classList.remove('busy');
      if (j.auth) return;
      if (!j.ok) { W.toast(t('만들지 못했습니다', 'Could not create'), j.network ? t('PC 에 닿지 않습니다. 네트워크를 확인하세요.', 'Cannot reach the PC.') : j.error, { err: true }); return; }
      $('newname').value = '';
      $('newRow').hidden = true;
      dirty = false;
      loadList().then(function () { openCue(j.cue.id); });
    });
  };

  // ---------- 큐시트 열기 ----------
  function openCue(id, keepView) {
    if (dirty && cur && cur.id !== id && !confirm(t('저장하지 않은 고침이 있습니다. 버리고 다른 큐시트를 열까요?', 'You have unsaved changes. Discard them and open another cue sheet?'))) return Promise.resolve();
    setState('busy', t('불러오는 중…', 'Loading…'));
    if (!cur || cur.id !== id) {
      // 다른 큐시트: 불러오는 동안 옛 표를 비워 둔다 (느린 Wi-Fi 에서 옛 표를 고치는 일이 없게)
      cur = null; dirty = false;
      $('items').innerHTML = '';
      $('editor').hidden = true;
      $('placeholder').hidden = false;
      $('placeholder').querySelector('b').textContent = t('불러오는 중…', 'Loading…');
      $('list').querySelectorAll('button[data-id]').forEach(function (b) { b.classList.toggle('on', b.dataset.id === id); });
    }
    var my = ++openCue.seq; // 늦게 온 옛 답이 나중에 누른 것을 덮지 않게
    var editsBefore = cur && cur.id === id ? sig(cur) : null;
    return api('/e/cue?id=' + encodeURIComponent(id)).then(function (j) {
      if (j.auth || my !== openCue.seq) return;
      // 불러오는 사이 같은 큐시트를 고쳤으면 늦게 온 답으로 덮지 않는다 (다시 불러오기는 고친 것을 버린다고 물은 뒤에만)
      if (editsBefore !== null && cur && cur.id === id && sig(cur) !== editsBefore) { touch(); return; }
      if (!j.ok) { $('placeholder').querySelector('b').textContent = t('큐시트를 고르세요', 'Choose a cue sheet'); setState('err', t('불러오지 못함', 'Not loaded')); W.toast(t('큐시트를 불러오지 못했습니다', 'Could not load the cue sheet'), j.network ? t('PC 에 닿지 않습니다. 잠시 뒤 다시 누르세요.', 'Cannot reach the PC. Try again shortly.') : j.error, { err: true }); return; }
      var keep = keepView && cur && cur.id === j.cue.id;
      cur = j.cue;
      cur.items = cur.items || [];
      origItems = {};
      cur.items.forEach(function (it) { if (it.id) origItems[it.id] = sig(it); });
      orig = sig(cur);
      dirty = false; staleRemote = false; undo = [];
      if (!keep) { sel = -1; open = -1; }
      confirmDel = -1;
      if (sel >= cur.items.length) sel = -1;
      if (open >= cur.items.length) open = -1;
      banner(null);
      $('app').classList.add('editing');
      drawEditor();
      drawList();
      setState('', t('불러옴', 'Loaded'));
      if (!keep) $('edit').scrollTop = 0;
    });
  }
  openCue.seq = 0;
  function reloadDiscard() { if (cur) { dirty = false; openCue(cur.id, true); } }
  $('reload').onclick = function () {
    if (!cur) { loadList(); return; }
    if (dirty && !confirm(t('저장하지 않은 고침을 버리고 PC 에서 다시 불러올까요?', 'Discard your unsaved changes and reload from the PC?'))) return;
    reloadDiscard();
  };
  $('back').onclick = function () {
    if (dirty && !confirm(t('저장하지 않은 고침이 있습니다. 버리고 목록으로 갈까요?', 'You have unsaved changes. Discard them and go back?'))) return;
    dirty = false;
    cur = null;
    $('app').classList.remove('editing');
    $('editor').hidden = true;
    $('placeholder').hidden = false;
    $('placeholder').querySelector('b').textContent = t('큐시트를 고르세요', 'Choose a cue sheet');
    $('curName').textContent = '';
    drawList();
    setState('', '');
  };

  // ---------- 그리기 ----------
  function sig(x) { return JSON.stringify(x); }
  function drawEditor() {
    $('placeholder').hidden = !!cur;
    $('editor').hidden = !cur;
    $('addPane').hidden = !cur;
    if (!cur) return;
    $('curName').textContent = cur.name;
    $('cname').value = cur.name || '';
    $('cdate').value = /^\d{4}-\d{2}-\d{2}$/.test(cur.serviceDate || '') ? cur.serviceDate : '';
    $('cmemo').value = cur.memo || '';
    grow($('cmemo'));
    drawItems();
  }
  function itemHtml(x, k) {
    var isSel = k === sel, isOpen = k === open;
    var changed = x.id && origItems[x.id] && origItems[x.id] !== sig(x);
    var h = '<div class="item' + (isSel ? ' sel' : '') + (isOpen ? ' open' : '') + (!x.id ? ' new' : '') + (changed ? ' changed' : '') + '" data-k="' + k + '" data-row="' + k + '" role="listitem">' +
      '<div class="head"><span class="n">' + (k + 1) + '</span>' +
      '<span class="ty">' + W.typeIcon(x.type) + esc(TYPES[x.type] || x.type) + '</span>' +
      '<input class="fld" data-f="title" value="' + esc(x.title) + '" maxlength="300" aria-label="' + t('제목', 'Title') + ' ' + (k + 1) + '" placeholder="' + t('제목', 'Title') + '">' +
      (x.note && !isOpen ? '<span class="notep">' + esc(x.note) + '</span>' : '') +
      '<span class="tools">' +
      '<button class="btn ghost icon-only" data-mv="-1" ' + (k === 0 ? 'disabled' : '') + ' aria-label="' + t('위로', 'Move up') + '" title="' + t('위로 (Alt+↑)', 'Move up (Alt+↑)') + '">' + W.icon('arrow-up') + '</button>' +
      '<button class="btn ghost icon-only" data-mv="1" ' + (k === cur.items.length - 1 ? 'disabled' : '') + ' aria-label="' + t('아래로', 'Move down') + '" title="' + t('아래로 (Alt+↓)', 'Move down (Alt+↓)') + '">' + W.icon('arrow-down') + '</button>' +
      '<button class="btn ghost icon-only" data-del="1" aria-label="' + t('지우기', 'Delete') + '" title="' + t('지우기 (Delete)', 'Delete (Delete)') + '">' + W.icon('trash-2') + '</button>' +
      '<button class="btn ghost icon-only open" data-open="1" aria-expanded="' + isOpen + '" aria-label="' + t('펼치기', 'Expand') + '" title="' + t('펼치기 (Enter)', 'Expand (Enter)') + '">' + W.icon('chevron-down') + '</button>' +
      '</span></div>';
    if (k === confirmDel) {
      h += '<div class="confirm"><span>' + t("'" + (x.title || '') + "' 순서를 지울까요?", "Delete '" + (x.title || '') + "'?") + '</span>' +
        '<button class="btn flat" data-delno="1">' + t('그만두기', 'Cancel') + '</button><button class="btn warn" data-delyes="1">' + t('지우기', 'Delete') + '</button></div>';
    }
    if (isOpen) {
      h += '<div class="body">';
      (FIELDS[x.type] || []).forEach(function (f) {
        var idf = 'f' + k + f[0];
        h += '<div class="frm"><label for="' + idf + '">' + esc(f[1]) + '</label>' + (f[2]
          ? '<textarea class="fld" id="' + idf + '" data-f="' + f[0] + '" rows="6" placeholder="' + esc(f[3]) + '">' + esc(x[f[0]]) + '</textarea>'
          : '<input class="fld" id="' + idf + '" data-f="' + f[0] + '" value="' + esc(x[f[0]]) + '" maxlength="300" placeholder="' + esc(f[3]) + '">') + '</div>';
      });
      h += '<div class="frm"><label for="f' + k + 'note">' + t('진행 메모', 'Cue note') + '</label><input class="fld" id="f' + k + 'note" data-f="note" value="' + esc(x.note) + '" maxlength="300" placeholder="' + t('화면에는 나가지 않음 (예: 인도자 신호 뒤)', 'Not shown on screen (e.g. after the leader’s cue)') + '"></div>';
      if (!FIELDS[x.type] && x.type !== 'blank') h += '<p class="note">' + t('이 순서의 내용(그림 · PPT · 영상 등)은 조작 화면에서 고칩니다. 여기서는 제목과 메모만 바꿉니다.', 'The content of this item (images, PPT, video…) is edited on the control PC. Only the title and note can be changed here.') + '</p>';
      if (x.type === 'song' && x.songId) h += '<p class="note">' + t('찬양 라이브러리와 이어진 곡입니다.', 'Linked to the song library.') + (x.credit ? ' ' + esc(x.credit) : '') + '</p>';
      h += '</div>';
    }
    return h + '</div>';
  }
  function drawItems(focus) {
    var it = cur.items;
    $('count').textContent = it.length + t('개', it.length === 1 ? ' item' : ' items');
    $('itemsEmpty').hidden = it.length > 0;
    $('items').innerHTML = it.map(itemHtml).join('');
    $('items').querySelectorAll('textarea').forEach(grow);
    if (focus) {
      var row = $('items').querySelector('.item[data-k="' + focus.k + '"]');
      var el = row && row.querySelector(focus.sel);
      if (el) { try { el.focus({ preventScroll: true }); } catch (_) { el.focus(); } }
      if (row) row.scrollIntoView({ block: 'nearest' });
    }
  }
  function grow(ta) { if (!ta || ta.tagName !== 'TEXTAREA') return; ta.style.height = 'auto'; ta.style.height = Math.min(Math.max(ta.scrollHeight + 2, ta.id === 'cmemo' ? 32 : 120), 520) + 'px'; }

  // ---------- 고침 표시 ----------
  function setState(kind, text) {
    var s = $('state');
    s.className = 'state' + (kind ? ' ' + kind : '');
    $('msg').textContent = text || '';
    $('stateDot').className = 'dot-s';
  }
  function touch() {
    dirty = !!cur && sig(cur) !== orig;
    $('save').disabled = !dirty || saving;
    if (saving) return;
    if (dirty) setState('dirty', t('고친 것 있음', 'Edited'));
    else if (lastSaved) setState('saved', t('저장됨 · ', 'Saved · ') + hm(lastSaved));
    else setState('', t('불러옴', 'Loaded'));
  }
  function hm(ms) { var d = new Date(ms); return (d.getHours() < 10 ? '0' : '') + d.getHours() + ':' + (d.getMinutes() < 10 ? '0' : '') + d.getMinutes(); }
  function snapshot() { undo.push(sig(cur.items)); if (undo.length > 60) undo.shift(); }
  function doUndo() {
    if (!undo.length) return false;
    cur.items = JSON.parse(undo.pop());
    confirmDel = -1;
    if (sel >= cur.items.length) sel = cur.items.length - 1;
    open = -1;
    drawItems();
    touch();
    return true;
  }

  // ---------- 입력 ----------
  $('editor').addEventListener('input', function (e) {
    var el = e.target;
    if (!cur) return;
    if (el.id === 'cname') { cur.name = el.value; $('curName').textContent = el.value; }
    else if (el.id === 'cdate') cur.serviceDate = el.value;
    else if (el.id === 'cmemo') { cur.memo = el.value; grow(el); }
    else if (el.dataset.f) {
      var row = el.closest('.item'); if (!row) return;
      var k = +row.dataset.k;
      cur.items[k][el.dataset.f] = el.value;
      if (el.tagName === 'TEXTAREA') grow(el);
      var changed = cur.items[k].id && origItems[cur.items[k].id] && origItems[cur.items[k].id] !== sig(cur.items[k]);
      row.classList.toggle('changed', !!changed);
    } else return;
    touch();
  });
  $('editor').addEventListener('focusin', function (e) {
    var row = e.target.closest && e.target.closest('.item');
    if (row && +row.dataset.k !== sel) select(+row.dataset.k, true);
  });
  function select(k, quiet) {
    sel = k;
    $('items').querySelectorAll('.item').forEach(function (r) { r.classList.toggle('sel', +r.dataset.k === k); });
    if (!quiet) { var r = $('items').querySelector('.item[data-k="' + k + '"]'); if (r) r.scrollIntoView({ block: 'nearest' }); }
  }
  function move(k, d) {
    var j = k + d;
    if (j < 0 || j >= cur.items.length) return;
    snapshot();
    var a = cur.items;
    var x = a[k]; a[k] = a[j]; a[j] = x;
    if (open === k) open = j; else if (open === j) open = k;
    sel = j;
    confirmDel = -1;
    drawItems({ k: j, sel: '[data-mv="' + d + '"]' });
    touch();
  }
  function del(k) {
    snapshot();
    var gone = cur.items.splice(k, 1)[0];
    confirmDel = -1;
    if (open === k) open = -1; else if (open > k) open--;
    sel = Math.min(k, cur.items.length - 1);
    drawItems();
    touch();
    W.toast(t('순서를 지웠습니다', 'Item deleted'), (gone.title || '') + t(' · 저장해야 PC 에 들어갑니다', ' · Save to apply on the PC'), { icon: 'trash-2', key: 'del', ms: 6000, action: { label: t('되돌리기', 'Undo'), fn: doUndo } });
  }
  function toggleOpen(k, focusField) {
    open = open === k ? -1 : k;
    sel = k;
    confirmDel = -1;
    drawItems(open === k && focusField ? { k: k, sel: '.body .fld' } : { k: k, sel: '[data-open]' });
  }
  $('items').addEventListener('click', function (e) {
    var row = e.target.closest('.item'); if (!row) return;
    var k = +row.dataset.k;
    var b = e.target.closest('button');
    if (b && b.dataset.mv) { move(k, +b.dataset.mv); return; }
    if (b && b.dataset.del) { confirmDel = confirmDel === k ? -1 : k; sel = k; drawItems({ k: k, sel: '[data-delyes]' }); return; }
    if (b && b.dataset.delyes) { del(k); return; }
    if (b && b.dataset.delno) { confirmDel = -1; drawItems({ k: k, sel: '[data-del]' }); return; }
    if (b && b.dataset.open) { toggleOpen(k); return; }
    if (e.target.closest('input, textarea, .body, .confirm')) return;
    toggleOpen(k);
  });

  // 순서 넣기
  $('adds').innerHTML = NEW.map(function (ty) { return '<button class="btn flat" data-add="' + ty + '">' + W.typeIcon(ty) + '<span>' + esc(TYPES[ty]) + '</span></button>'; }).join('');
  function addItem(x) {
    snapshot();
    cur.items.push(x);
    var k = cur.items.length - 1;
    sel = k;
    open = k;
    confirmDel = -1;
    drawItems({ k: k, sel: FIELDS[x.type] && x.type !== 'song' ? '.body .fld' : '[data-f="title"]' });
    touch();
  }
  $('adds').onclick = function (e) {
    var b = e.target.closest('[data-add]'); if (!b || !cur) return;
    var ty = b.dataset.add;
    addItem({ type: ty, title: TYPES[ty], subtitle: '', lyrics: '', ref: '', body: '', note: '', order: '' });
  };
  var songTimer = 0, songSeq = 0;
  $('songq').addEventListener('input', function () {
    clearTimeout(songTimer);
    var q = $('songq').value.trim();
    if (!q) { $('songs').innerHTML = ''; return; }
    $('songs').innerHTML = '<div class="none">' + t('찾는 중…', 'Searching…') + '</div>';
    songTimer = setTimeout(function () {
      var my = ++songSeq;
      api('/e/songs?q=' + encodeURIComponent(q)).then(function (j) {
        if (my !== songSeq || j.auth) return;
        if (!j.ok) { $('songs').innerHTML = '<div class="none">' + (j.network ? t('PC 에 닿지 않아 찾지 못했습니다.', 'Cannot reach the PC.') : esc(j.error || '')) + '</div>'; return; }
        $('songs').innerHTML = (j.songs || []).map(function (s) {
          return '<button data-song="' + esc(s.id) + '"><span><b>' + esc(s.title) + '</b><small>' + esc([s.key ? 'Key ' + s.key : '', s.credit].filter(Boolean).join(' · ') || ' ') + '</small></span>' + W.icon('plus') + '</button>';
        }).join('') || '<div class="none">' + t("'" + q + "' 에 맞는 곡이 없습니다. 초성(예: ㅈㅇㅎ)이나 제목 일부로 찾아 보세요.", "No songs match '" + q + "'. Try part of the title.") + '</div>';
      });
    }, 250);
  });
  $('songs').onclick = function (e) {
    var b = e.target.closest('[data-song]'); if (!b || !cur) return;
    b.classList.add('busy');
    api('/e/song', { id: b.dataset.song }).then(function (j) {
      b.classList.remove('busy');
      if (j.auth) return;
      if (!j.ok) { W.feedback(b, false); W.toast(t('곡을 넣지 못했습니다', 'Could not add the song'), j.network ? t('PC 에 닿지 않습니다.', 'Cannot reach the PC.') : j.error, { err: true }); return; }
      var x = j.item;
      W.feedback(b, true);
      addItem({ type: 'song', title: x.title, lyrics: x.lyrics || '', order: x.order || '', note: x.note || '', songId: x.songId || '', credit: x.credit || '' });
    });
  };

  // ---------- 저장 ----------
  function save(force) {
    if (!cur || saving || (!dirty && !force)) return;
    saving = true;
    $('save').disabled = true;
    $('save').classList.add('busy');
    setState('busy', t('보내는 중…', 'Sending…'));
    var body = { id: cur.id, base: cur.updatedAt, name: cur.name, serviceDate: cur.serviceDate, memo: cur.memo, items: cur.items };
    var pre = force ? api('/e/cue?id=' + encodeURIComponent(cur.id)).then(function (j) { if (j.ok) body.base = j.cue.updatedAt; return j; }) : Promise.resolve({ ok: true });
    pre.then(function () { return api('/e/save', body); }).then(function (j) {
      saving = false;
      $('save').classList.remove('busy');
      if (j.auth) return;
      if (j.ok) {
        var keepOpen = open, keepSel = sel;
        cur = j.cue;
        cur.items = cur.items || [];
        origItems = {};
        cur.items.forEach(function (it) { if (it.id) origItems[it.id] = sig(it); });
        orig = sig(cur);
        lastSaved = Date.now();
        staleRemote = false;
        open = keepOpen < cur.items.length ? keepOpen : -1;
        sel = keepSel < cur.items.length ? keepSel : -1;
        banner(null);
        drawItems();
        $('curName').textContent = cur.name;
        touch();
        $('msg').textContent = t('저장했습니다 · ', 'Saved · ') + hm(lastSaved);
        W.feedback($('save'), true);
        loadList();
        return;
      }
      W.feedback($('save'), false);
      touch();
      if (j.conflict || j.status === 409) {
        setState('err', t('충돌 · 저장 안 됨', 'Conflict · not saved'));
        banner('err', t('그 사이 다른 곳에서 이 큐시트를 고쳤습니다', 'Someone changed this cue sheet in the meantime'),
          t('조작 화면이나 다른 브라우저의 고침을 덮어쓰지 않으려고 저장하지 않았습니다. 내 고침은 이 화면에 그대로 있습니다.', 'Nothing was overwritten. Your changes are still on this screen.'),
          [[t('최신으로 다시 불러오기', 'Load the latest'), reloadDiscard, 'send'], [t('내 것으로 덮어쓰기…', 'Overwrite with mine…'), function () {
            if (confirm(t('다른 곳에서 고친 것을 지우고 이 화면의 큐시트로 저장할까요?', 'Replace the other changes with this version?'))) save(true);
          }, 'flat']]);
        return;
      }
      setState('err', t('저장 못 함', 'Not saved'));
      banner('err', t('저장하지 못했습니다', 'Could not save'),
        j.network ? t('PC 에 닿지 않습니다. 네트워크와 PC 의 워십프리젠터를 확인하세요. 고친 것은 이 화면에 그대로 있습니다.', 'Cannot reach the PC. Check the network and Worship Presenter. Your changes are still here.') : (j.error || '') + t(' 고친 것은 이 화면에 그대로 있습니다.', ' Your changes are still here.'),
        [[t('다시 저장', 'Save again'), function () { save(); }, 'send']]);
    });
  }
  $('save').onclick = function () { save(); };

  function banner(kind, title, text, acts) {
    var b = $('banner');
    if (!kind) { b.hidden = true; b.innerHTML = ''; return; }
    b.hidden = false;
    b.className = 'banner' + (kind === 'info' ? ' info' : '');
    b.innerHTML = W.icon(kind === 'info' ? 'info' : 'circle-alert') + '<div><b>' + esc(title) + '</b><span>' + esc(text) + '</span></div><div class="acts">' +
      (acts || []).map(function (a, i) { return '<button class="btn ' + (a[2] || '') + '" data-i="' + i + '">' + esc(a[0]) + '</button>'; }).join('') + '</div>';
    b.querySelectorAll('[data-i]').forEach(function (x) { x.onclick = function () { acts[+x.dataset.i][1](); }; });
  }

  // ---------- 키보드 ----------
  document.addEventListener('keydown', function (e) {
    if ($('app').hidden || !cur) return;
    var typing = e.target && /^(INPUT|TEXTAREA|SELECT)$/.test(e.target.tagName);
    var mod = e.ctrlKey || e.metaKey;
    if (mod && (e.key === 's' || e.key === 'S')) { e.preventDefault(); save(); return; }
    if (mod && !e.shiftKey && (e.key === 'z' || e.key === 'Z') && !typing) { if (doUndo()) e.preventDefault(); return; }
    if (e.altKey && (e.key === 'ArrowUp' || e.key === 'ArrowDown') && sel >= 0) { e.preventDefault(); move(sel, e.key === 'ArrowUp' ? -1 : 1); return; }
    if (e.key === 'Escape') {
      if (confirmDel >= 0) { confirmDel = -1; drawItems({ k: sel, sel: '[data-del]' }); }
      else if (open >= 0) { var k = open; open = -1; drawItems({ k: k, sel: '[data-open]' }); }
      else if (typing) e.target.blur();
      return;
    }
    if (typing) return;
    if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
      if (!cur.items.length) return;
      e.preventDefault();
      select(Math.max(0, Math.min(cur.items.length - 1, sel + (e.key === 'ArrowDown' ? 1 : -1))));
      return;
    }
    if (e.key === 'Enter' && sel >= 0) { e.preventDefault(); toggleOpen(sel, true); return; }
    if ((e.key === 'Delete' || e.key === 'Backspace') && sel >= 0) { e.preventDefault(); confirmDel = sel; drawItems({ k: sel, sel: '[data-delyes]' }); return; }
    if ((e.key === 'r' || e.key === 'R') && !mod) { $('reload').click(); }
  });

  $('toAdd').onclick = function () { $('addPane').scrollIntoView({ block: 'start', behavior: 'smooth' }); };

  // 좁은 화면: 순서 넣기 판을 표 아래로 옮긴다
  var mq = window.matchMedia('(max-width: 1179px)');
  function place() { var p = $('addPane'); if (mq.matches) { if (p.parentNode !== $('editor')) $('editor').appendChild(p); } else if (p.parentNode !== document.querySelector('.cols')) document.querySelector('.cols').appendChild(p); }
  if (mq.addEventListener) mq.addEventListener('change', place); else mq.addListener(place);
  place();

  window.addEventListener('beforeunload', function (e) { if (dirty) { e.preventDefault(); e.returnValue = ''; } });
  setInterval(function () { if (!$('app').hidden && document.visibilityState === 'visible' && !saving) loadList(); }, 10000);
  document.addEventListener('visibilitychange', function () { if (document.visibilityState === 'visible' && !$('app').hidden) loadList(); });

  // 시작: PIN 이 없어도 되는 이 PC 라면 바로 열린다 (401 이면 PIN 화면)
  loadList(false);
})();
