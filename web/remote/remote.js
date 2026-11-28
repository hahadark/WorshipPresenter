/* 휴대폰 리모컨 · OBS 도킹 (crates/wp-core/src/remote.rs)
 * - PIN 으로 연결 → 1초마다 상태(/r/state)를 받아 그림. 끊기면 점점 늦춰 가며 저절로 다시 붙는다.
 * - 조작(/r/do)은 한 번만 보내고 차례대로 (빨리 두 번 누른 '다음' 도 순서대로). 실패하면 무슨 일 + 어떻게 할지.
 * - 사고 막기: 순서 목록은 누르면 '다음'에 올리고 [송출] 로 한 번 더, '모두 끄기' 는 길게 누르기, 숨김과 양 끝으로.
 * 서버 약속(주소 · PIN · 필드)은 예전 화면과 같다. live.kind · nextItem · cueId · cue.id · note · app 은 덧붙인 필드(없어도 됨).
 */
(function () {
  'use strict';
  var $ = function (id) { return document.getElementById(id); };
  var W = window.WPW, t = W.t, esc = W.esc;
  var DOCK = location.pathname.indexOf('/dock') === 0; // OBS 도킹 (이 PC 안): PIN 없음
  var pin = DOCK ? 'dock' : W.session('wc-pin');

  var last = null;        // 마지막으로 받은 상태
  var recvAt = 0;         // 그 상태를 받은 때 (타이머 남은 시간 계산)
  var connected = false, everConnected = false, fails = 0, lastOk = 0;
  var pollTimer = 0, inflight = false, generation = 0;
  var tab = W.session('wp-remote-tab') || 'cue';
  var armed = -1, armTimer = 0; // 순서 목록에서 '다음'에 올린 줄
  var outSel = '';                // 화면 구성: 고른 출력
  var hold = false;               // 화면 구성 '계속 쓰기'
  var lockUntil = 0;              // PIN 을 여러 번 틀려 막힌 때까지

  // ---------- 처음 ----------
  if (DOCK) {
    document.body.classList.add('dock');
    $('dockObs').hidden = false;
    $('menuBtn').hidden = true;
  } else {
    W.mobileGuard(onBack);
  }

  // ---------- 보조 ----------
  function hexMix(a, b, k) { // a 를 k(0~1) 만큼, 나머지는 b (조작 화면 theme::mix 와 같은 셈)
    var pa = parseInt(a.slice(1), 16), pb = parseInt(b.slice(1), 16);
    var ch = function (sh) { return Math.round(((pa >> sh) & 255) * k + ((pb >> sh) & 255) * (1 - k)); };
    return 'rgb(' + ch(16) + ',' + ch(8) + ',' + ch(0) + ')';
  }
  function groupColor(k) {
    var c = window.BCGroup && window.BCGroup.color ? window.BCGroup.color(k) : '';
    return /^#[0-9a-f]{6}$/i.test(c || '') ? hexMix(c, '#c4c4c8', 0.45) : '';
  }
  function pad2(n) { return n < 10 ? '0' + n : '' + n; }
  function clock(ms) {
    var neg = ms < 0, s = Math.round(Math.abs(ms) / 1000), h = Math.floor(s / 3600), m = Math.floor(s / 60) % 60;
    return (neg ? '-' : '') + (h ? h + ':' + pad2(m) : m) + ':' + pad2(s % 60);
  }
  function setOnce(el, key, html) { if (el.getAttribute('data-k') !== key) { el.setAttribute('data-k', key); el.innerHTML = html; } }
  var statusTimer = 0;
  function status(text, err) {
    var e = $('actionStatus');
    e.textContent = text || '';
    e.classList.toggle('err', !!err);
    clearTimeout(statusTimer);
    if (text && !err) statusTimer = setTimeout(function () { e.textContent = ''; }, 1800);
  }

  // ---------- PIN ----------
  function cells() {
    var v = $('pin').value, cs = $('cells').children;
    for (var i = 0; i < cs.length; i++) {
      cs[i].textContent = v[i] || '';
      cs[i].classList.toggle('cur', i === Math.min(v.length, 5));
    }
  }
  function loginMsg(title, how, err) {
    $('loginErr').className = 'msg' + (err ? ' err' : '');
    $('loginErr').innerHTML = title ? '<b>' + esc(title) + '</b>' + (how ? '<span>' + esc(how) + '</span>' : '') : '';
    $('pinbox').classList.toggle('err', !!err);
  }
  function showLogin(title, how) {
    generation++;
    clearTimeout(pollTimer);
    connected = false;
    $('main').hidden = true;
    closeMenu();
    $('login').hidden = false;
    $('go').disabled = false;
    $('go').classList.remove('busy');
    $('go').textContent = t('연결', 'Connect');
    $('pin').value = '';
    cells();
    loginMsg(title, how, !!title);
    if (lockUntil > Date.now()) lockTick();
    setTimeout(function () { try { $('pin').focus(); } catch (_) { /* 무시 */ } }, 50);
  }
  function lockTick() {
    var left = Math.ceil((lockUntil - Date.now()) / 1000);
    if (left > 0 && !$('login').hidden) {
      $('go').disabled = true;
      $('go').textContent = t('잠시 기다려 주세요 · ', 'Please wait · ') + left + t('초', 's');
      setTimeout(lockTick, 500);
    } else {
      $('go').disabled = false;
      $('go').textContent = t('연결', 'Connect');
    }
  }
  $('pin').addEventListener('input', function () {
    var v = $('pin').value.replace(/\D/g, '').slice(0, 6);
    if ($('pin').value !== v) $('pin').value = v;
    $('pinbox').classList.remove('err');
    cells();
    if (v.length === 6 && lockUntil <= Date.now()) submit();
  });
  $('pin').addEventListener('focus', function () { $('pinbox').classList.add('focus'); });
  $('pin').addEventListener('blur', function () { $('pinbox').classList.remove('focus'); });
  $('pin').addEventListener('keydown', function (e) { if (e.key === 'Enter') submit(); });
  $('go').onclick = submit;
  function submit() {
    var v = $('pin').value.trim();
    if (!/^\d{6}$/.test(v)) { loginMsg(t('숫자 6자리를 넣어 주세요.', 'Enter all 6 digits.'), '', true); return; }
    if ($('go').disabled) return;
    pin = v;
    $('go').disabled = true;
    $('go').classList.add('busy');
    $('go').textContent = t('연결하는 중…', 'Connecting…');
    loginMsg('', '');
    start(true);
  }

  // ---------- 상태 받기 (끊기면 저절로 다시) ----------
  function start(fromLogin) {
    generation++;
    clearTimeout(pollTimer);
    fails = 0;
    if (!fromLogin) { $('login').hidden = true; $('main').hidden = false; }
    poll();
  }
  function schedule(ms) { clearTimeout(pollTimer); pollTimer = setTimeout(poll, ms); }
  function pollNow() { if (!inflight) { clearTimeout(pollTimer); poll(); } }
  function poll() {
    if (inflight) return;
    inflight = true;
    var gen = generation;
    W.request('/r/state?pin=' + encodeURIComponent(pin), null, 5000).then(function (r) {
      inflight = false;
      if (gen !== generation) return;
      var onLogin = !$('login').hidden;
      if (r.status === 401 || r.status === 429) {
        W.session('wc-pin', null);
        if (r.status === 429) {
          lockUntil = Date.now() + 30000;
          showLogin(t('PIN 을 여러 번 틀렸습니다', 'Too many wrong PINs'), t('30초 뒤에 다시 넣어 주세요. 숫자는 조작 PC 의 연결 → 휴대폰 리모컨 창에 있습니다.', 'Try again in 30 seconds. The number is in Connect → Phone remote on the control PC.'));
        } else if (onLogin) {
          showLogin(t('PIN 이 맞지 않습니다', 'Wrong PIN'), t('조작 PC 의 연결 → 휴대폰 리모컨 창에서 숫자를 다시 확인하세요. 5번 틀리면 30초 동안 막힙니다.', 'Check the number in Connect → Phone remote on the control PC. Five wrong tries lock it for 30 seconds.'));
        } else {
          showLogin(t('PIN 이 바뀌었습니다', 'The PIN has changed'), t('조작 PC 에서 새 PIN 을 만들었습니다. 연결 → 휴대폰 리모컨 창의 숫자를 넣어 주세요.', 'A new PIN was made on the control PC. Enter the number shown in Connect → Phone remote.'));
        }
        return;
      }
      if (r.ok) {
        fails = 0;
        lastOk = Date.now();
        if (onLogin) {
          W.session('wc-pin', pin);
          $('login').hidden = true;
          $('main').hidden = false;
          $('go').classList.remove('busy');
          $('go').textContent = t('연결', 'Connect');
          $('go').disabled = false;
        }
        if (!DOCK) W.wake.start();
        setConnected(true);
        draw(r);
        schedule(document.visibilityState === 'visible' ? 1000 : 4000);
        return;
      }
      // 끊김 · 서버 오류
      fails++;
      if (onLogin) {
        $('go').classList.remove('busy');
        $('go').disabled = false;
        $('go').textContent = t('연결', 'Connect');
        loginMsg(t('PC 에 닿지 않습니다', 'Cannot reach the PC'), t('휴대폰이 PC 와 같은 Wi-Fi 인지, PC 에서 워십프리젠터와 연결 → 휴대폰 리모컨이 켜져 있는지 확인하세요.', 'Check that this phone is on the same Wi-Fi as the PC, and that Worship Presenter and Connect → Phone remote are on.'), true);
        return;
      }
      if (fails >= 2 || Date.now() - lastOk > 3000) setConnected(false);
      if (!everConnected) drawNoData();
      schedule(Math.min(1000 * fails, 4000));
    });
  }
  function setConnected(on) {
    if (on && !connected && everConnected) { W.toast(t('다시 연결됨', 'Reconnected'), '', { key: 'conn', ms: 1600 }); W.buzz('ok'); }
    if (!on && connected) W.buzz('err');
    connected = on;
    if (on) everConnected = true;
    $('lost').hidden = on;
    $('conn').classList.toggle('lost', !on);
    $('connDot').className = 'dot-s ' + (on ? 'ok' : 'warn');
    $('connText').textContent = on ? (DOCK ? t('이 PC', 'This PC') : t('연결됨', 'Connected')) : t('끊김', 'Offline');
    document.body.classList.toggle('offline', !on);
    if (last) draw(last, true);
  }
  $('retry').onclick = function () { fails = 0; pollNow(); };
  window.addEventListener('online', function () { pollNow(); });
  document.addEventListener('visibilitychange', function () { if (document.visibilityState === 'visible' && $('login').hidden) pollNow(); });

  // ---------- 그리기 ----------
  function drawNoData() {
    $('badge').innerHTML = '<i class="dot-s"></i><span>' + t('연결 기다리는 중', 'Waiting for the PC') + '</span>';
    $('title').textContent = '';
    $('text').className = 'rtext empty';
    $('text').textContent = t('PC 에 연결되면 지금 송출 중인 화면이 여기에 보입니다.', 'The live slide appears here once connected.');
  }
  function draw(r, again) {
    last = r;
    if (!again) recvAt = Date.now();
    var L = r.live || {};
    var stale = !connected;
    var on = !!(L.has && L.visible) && !stale, hiddenText = !!(L.has && !L.visible);
    var cue = r.cue, same = !!(cue && (!L.cueId || L.cueId === cue.id));

    // 머리 줄 · 큐시트 이름
    $('cueTop').textContent = cue ? cue.name : '';
    $('cueName').textContent = cue ? t('순서 ', '') + cue.items.length + t('개', cue.items.length === 1 ? ' item' : ' items') : t('열린 큐시트 없음', 'No cue sheet open');

    // 지금 송출 중
    var rd = $('readout');
    rd.classList.toggle('onair', on);
    rd.classList.toggle('hidden-text', hiddenText);
    rd.classList.toggle('stale', stale);
    $('badge').innerHTML = '<i class="dot-s"></i><span>' + (stale ? t('끊김 · 마지막으로 받은 화면', 'Offline · last known') : on ? t('송출 중', 'Live') : hiddenText ? t('글자 숨김 중', 'Text hidden') : t('송출 없음', 'Nothing live')) + '</span>';
    $('count').textContent = L.has && L.count ? (L.index + 1) + ' / ' + L.count : '';
    var no = same && L.cueIndex >= 0 ? '<span class="n">' + pad2(L.cueIndex + 1) + '</span>' : '';
    $('title').innerHTML = L.has ? no + '<span class="tt">' + esc(L.title || t('(제목 없음)', '(untitled)')) + '</span>' : '';
    var tx = $('text');
    if (!L.has) {
      tx.className = 'rtext empty';
      tx.removeAttribute('data-k');
      tx.textContent = cue ? t('아래 순서를 눌러 고른 뒤 [송출] 을 누르세요.', 'Tap an item below, then tap Send.') : t('조작 PC 에서 큐시트를 열면 순서가 여기에 보입니다.', 'Open a cue sheet on the control PC to see its items here.');
    } else if (DOCK && L.image) {
      tx.className = 'rtext img';
      setOnce(tx, 'img:' + L.image, '<img alt="" src="' + esc(L.image) + '">');
    } else if (/^(image|score|video|camera|blank)$/.test(L.kind || '')) {
      tx.className = 'rtext';
      tx.removeAttribute('data-k');
      tx.innerHTML = '<span class="kind">' + W.typeIcon(L.kind) + esc(L.text || '') + '</span>';
    } else {
      tx.className = 'rtext';
      tx.removeAttribute('data-k');
      tx.textContent = L.text || t('(글 없는 슬라이드)', '(slide without text)');
    }
    // 다음 (노랑): 다음 장 → 없으면 다음 순서 → 없으면 마지막
    var nb = $('nextBox');
    if (!L.has) { nb.className = 'rnext none'; $('nextLabel').textContent = t('다음', 'Next'); $('next').textContent = '—'; }
    else if (L.next) { nb.className = 'rnext'; $('nextLabel').textContent = t('다음', 'Next'); $('next').textContent = L.next; }
    else if (L.nextItem) { nb.className = 'rnext item'; $('nextLabel').textContent = t('다음 순서', 'Next item'); $('next').textContent = L.nextItem; }
    else { nb.className = 'rnext none'; $('nextLabel').textContent = t('다음', 'Next'); $('next').textContent = t('마지막 장입니다', 'This is the last slide'); }

    drawLabels(L, on);
    drawCue(cue, L, same);
    drawStage(r.stage || {});
    drawScenes(r.scenes);
    drawNames(r.names || { people: [] });

    // 아래 단추
    var xh = $('xHide');
    xh.disabled = !L.has;
    xh.classList.toggle('warn', hiddenText);
    xh.innerHTML = W.icon(hiddenText ? 'eye' : 'eye-off') + '<span>' + (hiddenText ? t('글자 보이기', 'Show text') : t('글자 숨김', 'Hide text')) + '</span>';
    if (r.obs) { $('dPip').classList.toggle('on', !!r.obs.pip); $('dFull').classList.toggle('on', !!r.obs.full); }
    if (r.control === false && !draw.noctl) { draw.noctl = true; W.toast(t('조작 화면이 꺼져 있습니다', 'The control screen is closed'), t('다음 · 이전은 되지만 큐시트 순서 송출은 되지 않습니다.', 'Next and Previous work, but sending cue items does not.'), { err: true, ms: 8000 }); }
    if (r.app && r.app.version) $('menuFoot').textContent = t('워십프리젠터 ', 'Worship Presenter ') + r.app.version + ' · ' + location.host;
  }

  function drawLabels(L, on) {
    var labels = L.labels || [], groups = [];
    labels.forEach(function (l, i) {
      var k = String(l || '').replace(/-\d+$/, '');
      if (k && !/^\d+$/.test(k) && (i === 0 || String(labels[i - 1] || '').replace(/-\d+$/, '') !== k)) groups.push({ k: k, i: i });
    });
    var html = groups.map(function (g, n) {
      var end = groups[n + 1] ? groups[n + 1].i : 1e9;
      var here = L.has && L.index >= g.i && L.index < end;
      var c = groupColor(g.k);
      return '<button data-goto="' + g.i + '" class="' + (here ? (on ? 'on' : 'at') : '') + '" aria-current="' + (here ? 'true' : 'false') + '">' + esc(g.k) + (c ? '<i class="gc" style="background:' + c + '"></i>' : '') + '</button>';
    }).join('');
    setOnce($('labels'), JSON.stringify([groups, L.index, on]), html);
    var cur = $('labels').querySelector('[aria-current="true"]');
    if (cur && $('labels').getAttribute('data-scrolled') !== String(L.index)) {
      $('labels').setAttribute('data-scrolled', String(L.index));
      try { cur.scrollIntoView({ block: 'nearest', inline: 'center' }); } catch (_) { /* 오래된 브라우저 */ }
    }
  }

  function drawCue(cue, L, same) {
    if (!cue) { setOnce($('cue'), 'none', '<div class="empty"><b>' + t('열린 큐시트가 없습니다', 'No cue sheet is open') + '</b>' + t('조작 PC 에서 큐시트를 열어 주세요.', 'Open a cue sheet on the control PC.') + '</div>'); return; }
    if (!cue.items.length) { setOnce($('cue'), 'empty', '<div class="empty"><b>' + t('순서가 없습니다', 'No items') + '</b>' + t('조작 PC 에서 순서를 넣으면 여기에 보입니다.', 'Items added on the control PC appear here.') + '</div>'); return; }
    if (armed >= cue.items.length) armed = -1;
    var liveIdx = same && L.has ? L.cueIndex : -1;
    var html = cue.items.map(function (it, i) {
      var live = i === liveIdx, sel = i === armed;
      var tag = sel ? '' : live ? '<span class="tag">' + (L.visible ? t('송출 중', 'Live') : t('숨김', 'Hidden')) + '</span>' : '';
      var go = sel ? '<span class="btn send go" data-send="' + i + '">' + (live ? t('처음부터', 'Restart') : t('송출', 'Send')) + '</span>' : '';
      return '<button class="crow' + (live ? ' live' : '') + (sel ? ' sel' : '') + '" data-cue="' + i + '" aria-pressed="' + sel + '">' +
        '<span class="n">' + (i + 1) + '</span>' + W.typeIcon(it.type) +
        '<span class="ct">' + esc(it.title || t('(제목 없음)', '(untitled)')) + (it.note ? '<small>' + esc(it.note) + '</small>' : '') + '</span>' + tag + go + '</button>';
    }).join('');
    setOnce($('cue'), JSON.stringify([cue.items, liveIdx, L.visible, armed, W.lang]), html);
  }
  function arm(i) {
    armed = i;
    clearTimeout(armTimer);
    if (i >= 0) armTimer = setTimeout(function () { arm(-1); }, 10000);
    if (last) drawCue(last.cue, last.live || {}, !!(last.cue && (!last.live.cueId || last.live.cueId === last.cue.id)));
  }

  function drawStage(st) {
    var m = st.message || '';
    var now = $('msgNow');
    now.className = 'now' + (m ? '' : ' off');
    now.innerHTML = m ? '<small>' + t('지금 무대 화면에 떠 있음', 'On the stage display now') + '</small>' + esc(m) : t('무대 화면에 띄운 말이 없습니다.', 'No message on the stage display.');
    drawTimer();
  }
  function timerNow() {
    var tm = (last && last.stage && last.stage.timer) || {};
    if (!(tm.duration > 0)) return null;
    var rem = (+tm.remaining || 0) - (tm.running ? Date.now() - recvAt : 0);
    return { running: !!tm.running, rem: rem };
  }
  function drawTimer() {
    var x = timerNow(), lcd = $('tmNow'), tg = $('tmToggle');
    if (!x) {
      lcd.textContent = '--:--';
      lcd.className = 'tlcd num idle';
      $('tmState').textContent = t('시작 전', 'Not started');
      tg.disabled = true;
      tg.setAttribute('data-op', 'pause');
      tg.innerHTML = W.icon('pause') + '<span>' + t('멈춤', 'Pause') + '</span>';
      return;
    }
    lcd.textContent = clock(x.rem);
    lcd.className = 'tlcd num' + (x.rem < 0 ? ' over' : x.running ? '' : ' paused');
    $('tmState').textContent = x.rem < 0 ? t('시간 지남', 'Over time') : x.running ? t('도는 중', 'Running') : t('멈춤', 'Paused');
    tg.disabled = false;
    tg.setAttribute('data-op', x.running ? 'pause' : 'resume');
    tg.innerHTML = W.icon(x.running ? 'pause' : 'play') + '<span>' + (x.running ? t('멈춤', 'Pause') : t('계속', 'Resume')) + '</span>';
    // 탭 머리에 작은 점: 무대 타이머가 도는 중 (회색 — 정보)
  }
  setInterval(function () { if (last && tab === 'stage') drawTimer(); }, 500);

  function drawScenes(sc) {
    var has = !!(sc && sc.outputs && sc.outputs.length);
    tabVisible('layout', has);
    if (!has) { setOnce($('scenes'), 'none', ''); return; }
    if (!sc.outputs.some(function (o) { return o.id === outSel; })) outSel = sc.outputs[0].id;
    var o = sc.outputs.filter(function (x) { return x.id === outSel; })[0];
    var opts = [{ id: 'full', name: t('전체 화면', 'Full screen') }].concat(sc.list || []);
    var html = '<div class="seg outsel" role="tablist">' + sc.outputs.map(function (x) {
      return '<button data-osel="' + esc(x.id) + '" aria-selected="' + (x.id === outSel) + '">' + esc(x.name) + '</button>';
    }).join('') + '</div>' +
      '<div class="scgrid">' + opts.map(function (x) {
        var cur = o.now === x.id, base = o.base === x.id;
        return '<button class="btn flat' + (cur ? ' cur' : '') + '" data-out="' + esc(o.id) + '" data-sc="' + esc(x.id) + '" aria-pressed="' + cur + '"><span>' + esc(x.name) +
          (base ? '<span class="b">' + t('기본', 'Default') + '</span>' : cur && o.once ? '<span class="b">' + t('다음 순서까지', 'Until next item') + '</span>' : '') + '</span></button>';
      }).join('') + '</div>' +
      (o.once ? '<button class="btn flat restore" data-out="' + esc(o.id) + '" data-clear="1">' + W.icon('undo-2') + '<span>' + t('기본 구성으로 되돌리기', 'Back to the default layout') + '</span></button>' : '');
    setOnce($('scenes'), JSON.stringify([sc, outSel, W.lang]), html);
  }

  function drawNames(nm) {
    var people = nm.people || [];
    tabVisible('names', people.length > 0);
    var onKey = nm.on ? nm.on.name + '|' + (nm.on.role || '') : '';
    $('nameOff').hidden = !nm.on;
    var html = people.map(function (p, i) {
      var on = onKey === p.name + '|' + (p.role || '');
      return '<button class="nrow' + (on ? ' on' : '') + '" data-name="' + i + '" aria-pressed="' + on + '">' + W.icon('user') +
        '<span>' + esc(p.name) + (p.role ? '<small>' + esc(p.role) + '</small>' : '') + '</span>' +
        '<span class="tag">' + (on ? t('화면에 나감 · 누르면 내림', 'On screen · tap to take down') : t('띄우기', 'Show')) + '</span></button>';
    }).join('');
    setOnce($('names'), JSON.stringify([people, onKey, W.lang]), html);
  }

  // ---------- 탭 ----------
  function tabVisible(name, on) {
    var b = document.querySelector('[data-tab="' + name + '"]');
    if (b.hidden === !on) return;
    b.hidden = !on;
    if (!on && tab === name) setTab('cue');
  }
  function setTab(name) {
    // 탭을 옮기면 '길게 누르세요' 같은 짧은 안내는 닫는다 (N1)
    document.querySelectorAll('.toasts.near .toast').forEach(function (x) { x.remove(); });
    var b = document.querySelector('[data-tab="' + name + '"]');
    if (!b || b.hidden) name = 'cue';
    tab = name;
    W.session('wp-remote-tab', name);
    document.querySelectorAll('[data-tab]').forEach(function (x) { x.setAttribute('aria-selected', String(x.getAttribute('data-tab') === name)); });
    document.querySelectorAll('[data-pane]').forEach(function (p) { p.hidden = p.getAttribute('data-pane') !== name; });
    $('panes').scrollTop = 0;
    if (name === 'stage') drawTimer();
  }
  setTab(tab);

  // ---------- 조작 보내기 (차례대로 한 번씩) ----------
  var chain = Promise.resolve();
  function act(body, btn) {
    if (!connected) {
      W.feedback(btn, false);
      W.toast(t('연결이 끊겨 보내지 못했습니다', 'Not sent — disconnected'), t('위 띠가 사라지면(다시 연결되면) 다시 눌러 주세요.', 'Tap again once the banner disappears (reconnected).'), { err: true, key: 'act' });
      return;
    }
    chain = chain.then(function () { return send(body, btn); });
  }
  function send(body, btn) {
    var slow = setTimeout(function () { if (btn) btn.classList.add('busy'); status(t('보내는 중…', 'Sending…')); }, 250);
    var headers = { 'Content-Type': 'application/json' };
    if (DOCK) headers['X-WorshipCue'] = '1';
    return W.request('/r/do', { method: 'POST', headers: headers, body: JSON.stringify(Object.assign({ pin: pin }, body)) }, 6000).then(function (r) {
      clearTimeout(slow);
      if (btn) btn.classList.remove('busy');
      if (r.ok) {
        W.feedback(btn, true);
        status(t('반영됨', 'Done'));
        pollNow();
        return;
      }
      W.feedback(btn, false);
      if (r.status === 401 || r.status === 429) { W.session('wc-pin', null); showLogin(t('다시 연결해 주세요', 'Please reconnect'), t('PIN 이 바뀌었거나 여러 번 틀려 막혔습니다.', 'The PIN changed or was locked after wrong tries.')); return; }
      if (r.network) {
        status(t('응답 없음', 'No answer'), true);
        W.toast(t('PC 가 답하지 않습니다', 'The PC did not answer'), t('보낸 것이 반영됐는지 위 송출 칸을 보고, 바뀌지 않았으면 다시 누르세요.', 'Check the live panel above; if it did not change, tap again.'), { err: true, key: 'act' });
        fails++;
        pollNow();
        return;
      }
      status(t('되지 않음', 'Failed'), true);
      W.toast(t('되지 않았습니다', 'That did not work'), r.error || t('잠시 뒤 다시 눌러 주세요.', 'Please try again in a moment.'), { err: true, key: 'act' });
    });
  }

  // ---------- 단추 ----------
  function go(dir, btn) { arm(-1); act({ action: dir }, btn); }
  $('bNext').onclick = function () { go('next', this); };
  $('bPrev').onclick = function () { go('prev', this); };
  $('xHide').onclick = function () { act({ action: 'toggle' }, this); };
  $('dPip').onclick = function () { act({ action: 'obsPip' }, this); };
  $('dFull').onclick = function () { act({ action: 'obsFull' }, this); };

  // 모두 끄기: 0.9초 길게 누르기 (짧게 누르면 안내만)
  (function () {
    var b = $('xOff'), timer = 0, done = false;
    b.insertAdjacentHTML('afterbegin', '<i class="fill" aria-hidden="true"></i>');
    function cancel() { clearTimeout(timer); b.classList.remove('holding'); }
    b.addEventListener('pointerdown', function (e) {
      if (e.button > 0 || b.disabled) return;
      done = false;
      b.classList.add('holding');
      W.buzz('tap');
      timer = setTimeout(function () {
        done = true;
        b.classList.remove('holding');
        act({ action: 'off' }, b);
      }, 900);
    });
    ['pointerup', 'pointercancel', 'pointerleave'].forEach(function (ev) { b.addEventListener(ev, cancel); });
    b.addEventListener('contextmenu', function (e) { e.preventDefault(); });
    b.addEventListener('click', function () {
      if (done) { done = false; return; }
      W.toast(t('모두 끄기는 길게 누르세요', 'Press and hold to turn everything off'), t('글 · 배경 · 로고와 예배당 화면까지 모두 꺼집니다. 1초쯤 누르고 있으면 꺼집니다.', 'Text, background, logo and the sanctuary screen all go off. Hold for about a second.'), { icon: 'power', key: 'off', ms: 2000, above: $('transport') });
    });
  })();

  document.addEventListener('click', function (e) {
    var b = e.target.closest && e.target.closest('button, [data-send]');
    if (!b || b.disabled) return;
    var d = b.dataset || {};
    if (d.send != null) { e.stopPropagation(); var i = +d.send; arm(-1); act({ action: 'cue', index: i }, b.closest('.crow')); return; }
    if (d.cue != null) { var k = +d.cue; W.buzz('tap'); arm(armed === k ? -1 : k); return; }
    if (d.goto != null) { act({ action: 'goto', index: +d.goto }, b); return; }
    if (d.tab) { setTab(d.tab); return; }
    if (d.osel) { outSel = d.osel; if (last) drawScenes(last.scenes); return; }
    if (d.out) { act(d.clear ? { action: 'scene', output: d.out, clear: true } : { action: 'scene', output: d.out, id: d.sc, hold: hold }, b); return; }
    if (d.name != null) { act({ action: 'name', index: b.classList.contains('on') ? -1 : +d.name }, b); return; }
    if (d.msg) { var text = b.textContent.trim(); $('msg').value = text; act({ action: 'message', text: text }, b); return; }
    if (d.tm) { act({ action: 'timer', op: 'start', minutes: +d.tm }, b); return; }
    if (d.add) { act({ action: 'timer', op: 'add', minutes: +d.add }, b); return; }
    if (d.op) { act({ action: 'timer', op: d.op }, b); return; }
  });
  $('nameOff').onclick = function () { act({ action: 'name', index: -1 }, this); };
  $('msgSend').onclick = function () {
    var v = $('msg').value.trim();
    if (!v) { W.feedback(this, false); W.toast(t('보낼 말을 적어 주세요', 'Type a message first'), t('말을 내리려면 [말 지우기] 를 누르세요.', 'To remove the message, tap Clear message.'), { err: true, key: 'msg' }); return; }
    act({ action: 'message', text: v }, this);
    try { $('msg').blur(); } catch (_) { /* 무시 */ }
  };
  $('msg').addEventListener('keydown', function (e) { if (e.key === 'Enter') { e.preventDefault(); $('msgSend').click(); } });
  $('msgClear').onclick = function () { $('msg').value = ''; act({ action: 'message', text: '' }, this); };
  $('tmAtGo').onclick = function () {
    var v = $('tmAt').value.trim();
    if (!v) { W.feedback(this, false); W.toast(t('끝낼 시각을 적어 주세요', 'Enter an end time'), t("예: 11:00 · 11시 30분", 'For example 11:00'), { err: true, key: 'tm' }); return; }
    act({ action: 'timer', op: 'until', at: v }, this);
    try { $('tmAt').blur(); } catch (_) { /* 무시 */ }
  };
  $('tmAt').addEventListener('keydown', function (e) { if (e.key === 'Enter') { e.preventDefault(); $('tmAtGo').click(); } });
  $('scHold').onclick = function () { hold = !hold; this.setAttribute('aria-checked', String(hold)); };

  // 키보드 (OBS 도킹 · 태블릿 자판): → · PageDown · Space = 다음, ← · PageUp = 이전
  document.addEventListener('keydown', function (e) {
    if ($('main').hidden || e.altKey || e.ctrlKey || e.metaKey) return;
    if (e.target && /^(INPUT|TEXTAREA|SELECT)$/.test(e.target.tagName)) return;
    if (e.key === 'ArrowRight' || e.key === 'PageDown' || e.key === ' ') { e.preventDefault(); go('next', $('bNext')); }
    else if (e.key === 'ArrowLeft' || e.key === 'PageUp') { e.preventDefault(); go('prev', $('bPrev')); }
    else if (e.key === 'Escape') { if (!$('menu').hidden) closeMenu(); else arm(-1); }
  });

  // ---------- 설정 판 ----------
  function openMenu() { $('menu').hidden = false; drawMenu(); }
  function closeMenu() { $('menu').hidden = true; }
  function drawMenu() {
    $('optDim').setAttribute('aria-checked', String(W.dim()));
    $('optVib').setAttribute('aria-checked', String(W.vibrate()));
    $('rowVib').hidden = !W.canVibrate;
    var s = W.wake.state();
    $('wakeText').textContent = W.wake.text(s);
    $('wakeHint').textContent = s === 'on' || s === 'video'
      ? t('예배 중에 이 화면이 꺼지지 않습니다. 배터리를 위해 충전기를 꽂아 두세요.', 'This screen will not turn off during the service. Keep a charger plugged in.')
      : t('이 브라우저에서는 막지 못할 수 있습니다. 휴대폰 설정에서 자동 잠금을 꺼 주세요.', 'This browser may not keep the screen on. Turn off auto-lock in the phone settings.');
    if (!$('menuFoot').textContent) $('menuFoot').textContent = location.host;
  }
  $('menuBtn').onclick = openMenu;
  document.querySelectorAll('#menu [data-close]').forEach(function (b) { b.onclick = closeMenu; });
  $('optDim').onclick = function () { W.dim(!W.dim()); drawMenu(); };
  $('optVib').onclick = function () { W.vibrate(!W.vibrate()); drawMenu(); };
  $('optWake').onclick = function () { W.wake.start(); setTimeout(drawMenu, 300); };
  $('optLogout').onclick = function () { W.session('wc-pin', null); pin = ''; showLogin(); };
  W.wake.on(function () { if (!$('menu').hidden) drawMenu(); });

  var backTold = false;
  function onBack() {
    if (!$('menu').hidden) { closeMenu(); return; }
    if (armed >= 0) { arm(-1); return; }
    if (!backTold) { backTold = true; W.toast(t('뒤로 가기를 막았습니다', 'Back is disabled here'), t('예배 중 실수로 리모컨이 닫히지 않게 했습니다. 끝내려면 브라우저 탭을 닫으세요.', 'So the remote does not close by accident. Close the browser tab to leave.'), { icon: 'info', ms: 4000 }); }
  }

  // ---------- 시작 ----------
  if (pin) { $('login').hidden = true; $('main').hidden = false; start(false); }
  else showLogin();
})();
