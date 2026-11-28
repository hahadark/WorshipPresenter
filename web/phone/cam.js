/* 휴대폰을 카메라로 (crates/wp-core/src/phonecam.rs)
 * 카메라 허락 → 0.7초마다 PC 에 '여기 있음'(/cam/poll) → PC 가 이 카메라를 고르면 offer 가 오고 WebRTC 로 보낸다.
 * 서버 약속(주소 · 열쇠 k · id · 필드)은 예전 화면과 같다. 답의 onAir(이 영상이 나가는 출력)는 덧붙인 필드.
 * 시험(tools/phonecam/e2e.js)이 보는 것: #dot 의 class(ready · on), #st 글, localStorage 'wp-cam-id'.
 */
(function () {
  'use strict';
  var $ = function (id) { return document.getElementById(id); };
  var W = window.WPW, t = W.t, esc = W.esc;
  W.mobileGuard(function () { if (!$('menu').hidden) closeMenu(); });

  var K = new URLSearchParams(location.search).get('k') || '';
  var id = W.local('wp-cam-id');
  if (!/^[A-Za-z0-9_-]{4,64}$/.test(id)) { id = 'p' + Math.random().toString(36).slice(2, 12); W.local('wp-cam-id', id); }
  var ua = navigator.userAgent;
  var guess = /iPhone/.test(ua) ? t('아이폰', 'iPhone') : /iPad/.test(ua) ? t('아이패드', 'iPad') : /Android/.test(ua) ? t('안드로이드', 'Android') : t('휴대폰', 'Phone');
  $('name').value = W.local('wp-cam-name') || guess;
  $('name').addEventListener('change', function () { W.local('wp-cam-name', $('name').value.trim()); W.toast(t('이름을 바꿨습니다', 'Name changed'), t('PC 카메라 목록에 곧 보입니다.', 'It will show in the PC camera list shortly.'), { key: 'name' }); });
  $('name').addEventListener('keydown', function (e) { if (e.key === 'Enter') $('name').blur(); });

  var facing = W.local('wp-cam-facing') === 'user' ? 'user' : 'environment';
  var hd = W.local('wp-cam-q') !== '720';
  var stream = null, opening = false, camErr = '';
  var pcs = {}; // 보는 화면(viewer) → RTCPeerConnection
  var lastOk = 0, linked = false, badKey = false, onAir = [], polls = 0;

  // ---------- 상태 그리기 ----------
  function liveCount() { var n = 0; for (var k in pcs) if (pcs[k].connectionState === 'connected') n++; return n; }
  function camName() { return $('name').value.trim() || guess; }
  function outName(x) { return x === 'main' ? t('예배당', 'Sanctuary') : x === 'overlay' ? 'OBS' : x; }
  function step(el, cls) { $(el).className = cls || ''; }
  function render() {
    var cam = !!(stream && stream.active), live = liveCount(), lost = !badKey && polls > 0 && Date.now() - lastOk > 4000;
    var app = $('app');
    app.classList.toggle('onair', cam && onAir.length > 0 && !lost);
    app.classList.toggle('lost', lost || badKey);
    // 세 단계
    step('s1', camErr ? 'bad' : cam ? 'done' : 'now');
    step('s2', !cam ? '' : badKey || lost ? 'bad' : linked ? 'done' : 'now');
    step('s3', !cam || !linked || lost ? '' : live ? 'done' : 'now');
    var st, sub = '', dot = '';
    if (camErr) { st = t('카메라를 열지 못했습니다', 'Camera is not open'); sub = t('아래 안내를 따라 주세요', 'Follow the steps below'); dot = 'warn'; }
    else if (!cam) { st = t('카메라 허락을 기다리는 중', 'Waiting for camera permission'); sub = t('휴대폰에 뜬 창에서 [허용] 을 누르세요', 'Tap Allow on the prompt'); }
    else if (badKey) { st = t('주소의 열쇠가 바뀌었습니다', 'This link is no longer valid'); sub = t('PC 의 QR 을 다시 찍으세요', 'Scan the QR on the PC again'); dot = 'warn'; }
    else if (lost) { st = t('PC 와 끊김 · 다시 연결하는 중', 'Lost the PC — reconnecting'); sub = t('같은 Wi-Fi 인지, PC 에서 워십프리젠터가 켜져 있는지 확인하세요', 'Check the Wi-Fi and that Worship Presenter is running'); dot = 'warn'; }
    else if (!linked) { st = t('PC 와 연결하는 중…', 'Connecting to the PC…'); sub = location.host; }
    else if (onAir.length) { st = t('송출 중', 'Live') + ' · ' + onAir.map(outName).join(' · '); sub = t('이 영상이 지금 나가고 있습니다. 휴대폰을 움직이지 마세요', 'This camera is on air — keep the phone still'); dot = 'on'; }
    else if (live) { st = t('PC 로 영상을 보내는 중', 'Sending video to the PC'); sub = t('PC 가 받고 있습니다 (아직 화면에 나가지는 않음)', 'The PC is receiving it (not on air yet)'); dot = 'sending'; }
    else { st = t('연결됨 · PC 에서 이 카메라를 고르세요', 'Connected — pick this camera on the PC'); sub = t('PC 카메라 목록 이름: ', 'Name in the PC list: ') + camName(); dot = 'ready'; }
    $('st').textContent = st;
    $('sub').textContent = sub;
    $('dot').className = 'dot ' + dot;
    // 미리보기 위 표시
    $('air').hidden = !app.classList.contains('onair');
    $('air').textContent = t('송출 중', 'LIVE') + ' · ' + onAir.map(outName).join(' · ');
    var s = settings();
    $('meta').hidden = !cam;
    $('meta').textContent = s.width ? s.width + '×' + s.height + (s.frameRate ? ' · ' + Math.round(s.frameRate) + 'fps' : '') : '';
    // 세로 영상 안내 (휴대폰을 세운 채면 PC 에 좁고 긴 영상)
    var portrait = s.width && s.height > s.width;
    var narrow = window.innerHeight > window.innerWidth;
    $('rotate').hidden = !cam || !(portrait || narrow);
    $('rotateText').textContent = portrait
      ? t('지금 세로 영상으로 나갑니다. 휴대폰을 가로로 눕히면 PC 화면에 꽉 차게 나갑니다.', 'The video is portrait. Turn the phone sideways to fill the PC screen.')
      : t('휴대폰을 가로로 눕혀 세워 두면 화면이 넓게 보입니다.', 'Turn the phone sideways for a wider view.');
  }
  function settings() {
    var tr = stream && stream.getVideoTracks()[0];
    try { return tr && tr.getSettings ? tr.getSettings() : {}; } catch (_) { return {}; }
  }
  function sizeText() { var s = settings(); return s.width ? s.width + 'x' + s.height : ''; }

  function hint(kind, title, html, button) {
    if (!kind) { $('hint').hidden = true; return; }
    $('hint').hidden = false;
    $('hint').className = 'hint' + (kind === 'err' ? ' err' : '');
    $('hintIcon').innerHTML = W.icon(kind === 'err' ? 'circle-alert' : 'camera');
    $('hintTitle').textContent = title;
    $('hintText').innerHTML = html || '';
    $('start').hidden = !button;
    if (button) $('start').textContent = button;
  }

  // ---------- 카메라 ----------
  function howToAllow() {
    if (/iPhone|iPad/.test(ua)) return t('<ol><li>주소창 왼쪽 <b>aA</b> 를 누르고</li><li><b>웹 사이트 설정 → 카메라 → 허용</b></li><li>아래 [다시 시도] 를 누르세요</li></ol>', '<ol><li>Tap <b>aA</b> at the left of the address bar</li><li><b>Website Settings → Camera → Allow</b></li><li>Tap Retry below</li></ol>');
    if (/Android/.test(ua)) return t('<ol><li>주소창 왼쪽 <b>자물쇠(설정)</b> 를 누르고</li><li><b>권한 → 카메라 → 허용</b></li><li>아래 [다시 시도] 를 누르세요</li></ol>', '<ol><li>Tap the <b>lock (settings)</b> icon left of the address</li><li><b>Permissions → Camera → Allow</b></li><li>Tap Retry below</li></ol>');
    return t('브라우저 주소창의 사이트 설정에서 카메라를 허용한 뒤 [다시 시도] 를 누르세요.', 'Allow the camera in the site settings of the address bar, then tap Retry.');
  }
  function open() {
    if (opening) return Promise.resolve(false);
    if (!navigator.mediaDevices || !navigator.mediaDevices.getUserMedia) {
      camErr = 'unsupported';
      hint('err', t('이 브라우저에서는 카메라를 쓸 수 없습니다', 'The camera is not available in this browser'),
        t('카카오톡 · 밴드 같은 앱 안에서 열렸다면, 휴대폰 <b>기본 카메라</b>로 QR 을 다시 찍어 <b>크롬(안드로이드)</b> 또는 <b>사파리(아이폰)</b> 에서 여세요.', 'If this opened inside a chat app, scan the QR again with the phone\'s <b>camera app</b> and open it in <b>Chrome (Android)</b> or <b>Safari (iPhone)</b>.'));
      render();
      return Promise.resolve(false);
    }
    opening = true;
    $('flip').disabled = true;
    // 아이폰은 쓰던 카메라를 놓아야 다른 카메라가 열린다
    if (stream) { stream.getTracks().forEach(function (x) { x.onended = null; x.stop(); }); }
    var w = hd ? 1920 : 1280, h = hd ? 1080 : 720;
    var want = { video: { facingMode: { ideal: facing }, width: { ideal: w }, height: { ideal: h }, frameRate: { ideal: 30 } }, audio: false };
    if (!stream) hint('wait', t('카메라 허락을 눌러 주세요', 'Allow the camera'), t('휴대폰에 뜬 창에서 <b>허용</b> 을 누르면 미리보기가 나옵니다. 소리(마이크)는 쓰지 않습니다.', 'Tap <b>Allow</b> on the prompt to see the preview. The microphone is not used.'));
    return navigator.mediaDevices.getUserMedia(want).catch(function (e) {
      // 해상도를 못 맞추는 오래된 기기: 조건을 풀어 한 번 더
      if (e && (e.name === 'OverconstrainedError' || e.name === 'ConstraintNotSatisfiedError')) return navigator.mediaDevices.getUserMedia({ video: { facingMode: facing }, audio: false });
      throw e;
    }).then(function (s) {
      stream = s;
      camErr = '';
      var v = $('v');
      v.srcObject = s;
      v.classList.toggle('mirror', facing === 'user'); // 앞 카메라 미리보기만 거울처럼 (보내는 영상은 그대로)
      var tr = s.getVideoTracks()[0];
      try { tr.contentHint = 'motion'; } catch (_) { /* 무시 */ }
      // 이미 보고 있는 화면은 새 카메라로 바꿔 끼운다 (다시 잇지 않고)
      for (var k in pcs) pcs[k].getSenders().forEach(function (snd) { if (snd.track && snd.track.kind === 'video') snd.replaceTrack(tr).catch(function () {}); });
      tr.onended = function () { setTimeout(function () { if (document.visibilityState === 'visible') open(); }, 800); }; // 다른 앱이 카메라를 가져갔다 돌려주면 다시
      hint('');
      var p = v.play();
      if (p && p.catch) p.catch(function () { hint('wait', t('미리보기를 누르면 보입니다', 'Tap to show the preview'), t('영상은 이미 준비됐습니다.', 'The video is ready.'), t('미리보기 보기', 'Show preview')); });
      W.local('wp-cam-facing', facing);
      $('flipText').textContent = facing === 'user' ? t('뒤 카메라로', 'Rear camera') : t('앞 카메라로', 'Front camera');
      W.wake.start();
      return true;
    }).catch(function (e) {
      var n = (e && e.name) || 'Error';
      camErr = n;
      if (n === 'NotAllowedError' || n === 'SecurityError') hint('err', t('카메라 허락이 꺼져 있습니다', 'Camera access is blocked'), howToAllow(), t('다시 시도', 'Retry'));
      else if (n === 'NotReadableError' || n === 'AbortError') hint('err', t('다른 앱이 카메라를 쓰고 있습니다', 'Another app is using the camera'), t('카메라 · 영상 통화 앱을 닫고 [다시 시도] 를 누르세요.', 'Close camera or video-call apps, then tap Retry.'), t('다시 시도', 'Retry'));
      else if (n === 'NotFoundError') hint('err', t('카메라를 찾지 못했습니다', 'No camera found'), t('이 기기에 쓸 수 있는 카메라가 없습니다.', 'This device has no usable camera.'), t('다시 시도', 'Retry'));
      else hint('err', t('카메라를 열지 못했습니다', 'Could not open the camera'), t('잠시 뒤 [다시 시도] 를 누르세요. 안 되면 이 페이지를 닫고 QR 을 다시 찍으세요.', 'Tap Retry in a moment. If it keeps failing, close this page and scan the QR again.') + ' <span class="ter">(' + esc(n) + ')</span>', t('다시 시도', 'Retry'));
      return false;
    }).then(function (ok) {
      opening = false;
      $('flip').disabled = false;
      render();
      return ok;
    });
  }

  // ---------- PC 와 잇기 ----------
  function post(url, body) {
    return W.request(url, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body) }, 6000);
  }
  function waitIce(pc, ms) {
    if (pc.iceGatheringState === 'complete') return Promise.resolve();
    return new Promise(function (res) {
      var done = function () { if (pc.iceGatheringState === 'complete') { pc.removeEventListener('icegatheringstatechange', done); res(); } };
      pc.addEventListener('icegatheringstatechange', done);
      setTimeout(res, ms);
    });
  }
  function onOffer(viewer, sdp) {
    if (!stream) return Promise.resolve();
    if (pcs[viewer]) pcs[viewer].close();
    var pc = new RTCPeerConnection({ iceServers: [] });
    pcs[viewer] = pc;
    var snd = pc.addTrack(stream.getVideoTracks()[0], stream);
    pc.onconnectionstatechange = function () {
      if ((pc.connectionState === 'failed' || pc.connectionState === 'closed') && pcs[viewer] === pc) { delete pcs[viewer]; pc.close(); }
      render();
    };
    return pc.setRemoteDescription({ type: 'offer', sdp: sdp })
      .then(function () { return pc.createAnswer(); })
      .then(function (a) { return pc.setLocalDescription(a); })
      .then(function () {
        // 화질을 지키고(해상도 유지) 넉넉한 비트레이트로: 같은 Wi-Fi 안이라 인터넷 속도와 상관없다
        try {
          var p = snd.getParameters();
          p.encodings = p.encodings && p.encodings.length ? p.encodings : [{}];
          p.encodings[0].maxBitrate = hd ? 8000000 : 4000000;
          p.degradationPreference = 'maintain-resolution';
          return snd.setParameters(p).catch(function () {});
        } catch (_) { return null; }
      })
      .then(function () { return waitIce(pc, 2000); })
      .then(function () { return post('/cam/answer', { k: K, id: id, viewer: viewer, sdp: pc.localDescription.sdp }); });
  }
  function poll() {
    return post('/cam/poll', { k: K, id: id, name: camName(), viewers: Object.keys(pcs).length, size: sizeText() }).then(function (r) {
      polls++;
      if (r.ok) {
        lastOk = Date.now();
        if (!linked) W.buzz('ok');
        linked = true;
        if (badKey) hint('');
        badKey = false;
        onAir = r.onAir || [];
        (r.msgs || []).forEach(function (m) {
          if (m.t === 'offer') onOffer(m.viewer, m.sdp).catch(function (e) { console.warn(e); });
          else if (m.t === 'bye' && pcs[m.viewer]) { pcs[m.viewer].close(); delete pcs[m.viewer]; }
        });
      } else if (r.status === 403 || r.error === 'key') {
        if (!badKey) hint('err', t('주소의 열쇠가 바뀌었습니다', 'This link is no longer valid'), t('PC 에서 휴대폰 카메라 주소를 새로 만들었습니다. PC 의 <b>연결 → 휴대폰 카메라</b> 창에 보이는 QR 을 휴대폰 기본 카메라로 다시 찍어 주세요.', 'A new phone-camera link was made on the PC. Scan the QR in <b>Connect → Phone camera</b> on the PC again with the phone camera app.'));
        badKey = true;
        linked = false;
        onAir = [];
      } else if (Date.now() - lastOk > 4000) {
        linked = false;
        onAir = [];
      }
      render();
    });
  }
  function loop() {
    var next = function () { setTimeout(loop, badKey ? 3000 : 700); };
    if (!stream || !stream.active) { render(); next(); return; }
    poll().then(next, next);
  }

  // ---------- 조작 ----------
  $('flip').onclick = function () { if (opening) return; facing = facing === 'environment' ? 'user' : 'environment'; open(); };
  function setQ(q) {
    hd = q !== '720';
    W.local('wp-cam-q', hd ? null : '720');
    $('res').className = hd ? 'on' : ''; $('res').setAttribute('aria-checked', String(hd));
    $('res720').className = hd ? '' : 'on'; $('res720').setAttribute('aria-checked', String(!hd));
  }
  $('res').onclick = function () { if (opening || hd) return; setQ('1080'); open(); };
  $('res720').onclick = function () { if (opening || !hd) return; setQ('720'); open(); };
  setQ(hd ? '1080' : '720');
  $('start').onclick = function () {
    if (stream && stream.active) { $('v').play().then(function () { hint(''); }, function () {}); return; }
    camErr = '';
    open();
  };
  function openMenu() { $('menu').hidden = false; $('optDim').setAttribute('aria-checked', String(W.dim())); }
  function closeMenu() { $('menu').hidden = true; }
  $('menuBtn').onclick = openMenu;
  document.querySelectorAll('#menu [data-close]').forEach(function (b) { b.onclick = closeMenu; });
  $('optDim').onclick = function () { W.dim(!W.dim()); openMenu(); };
  $('optCover').onclick = function () { closeMenu(); $('cover').hidden = false; };
  $('cover').onclick = function () { $('cover').hidden = true; };
  $('menuFoot').textContent = location.host;

  // 화면 꺼짐 방지 상태 한 줄
  W.wake.on(function (s) { document.querySelectorAll('[data-wake]').forEach(function (e) { e.textContent = s === 'off' ? t('화면 꺼짐 방지: 카메라가 켜지면 켜집니다', 'Keep-awake turns on with the camera') : W.wake.text(s); }); });
  // 배터리 (안드로이드 크롬만 알려 줌)
  if (navigator.getBattery) {
    navigator.getBattery().then(function (b) {
      var draw = function () {
        var low = b.level < 0.3 && !b.charging;
        $('battery').classList.toggle('warn', low);
        $('battery').lastChild.textContent = (b.charging ? t('충전 중 · ', 'Charging · ') : t('배터리 ', 'Battery ') + Math.round(b.level * 100) + '% · ') +
          (low ? t('곧 꺼질 수 있습니다. 충전기를 꽂아 주세요.', 'May run out soon — plug in a charger.') : t('뜨거워지면 720p 로 낮추세요.', 'If it gets hot, switch to 720p.'));
      };
      b.addEventListener('levelchange', draw); b.addEventListener('chargingchange', draw); draw();
    }).catch(function () {});
  }

  // 다른 앱에 갔다 오면 카메라를 다시 (아이폰은 화면을 떠나면 카메라를 끊는다)
  document.addEventListener('visibilitychange', function () {
    if (document.visibilityState === 'visible' && !opening && !camErr && (!stream || !stream.active)) open();
  });
  window.addEventListener('resize', render);
  window.addEventListener('pagehide', function () { for (var k in pcs) pcs[k].close(); pcs = {}; if (stream) stream.getTracks().forEach(function (x) { x.stop(); }); });

  // ---------- 시작 ----------
  if (!K) {
    camErr = 'nokey';
    hint('err', t('주소에 열쇠가 없습니다', 'This link is missing its key'), t('PC 의 <b>연결 → 휴대폰 카메라</b> 창에 보이는 QR 을 휴대폰 기본 카메라로 다시 찍어 주세요.', 'Scan the QR code in <b>Connect → Phone camera</b> on the PC with the phone\'s camera app.'));
    render();
    return;
  }
  render();
  open().then(function () { loop(); });
})();
