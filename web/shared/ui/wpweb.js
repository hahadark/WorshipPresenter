/* 워십프리젠터 웹 화면 공통 도구 — 휴대폰 리모컨(web/remote) · 휴대폰 카메라(web/phone) · 웹 편집기(web/editor)
 * 빌드 도구 · 바깥 주소 없이 (인터넷 없는 예배당 내부망). 오래된 휴대폰(iOS 13.4 · OBS 도킹 CEF 103)도 돌게 쉬운 문법만.
 *
 *   WPW.t('한국어', 'English')        화면 언어(앱 설정 → <html data-app-lang>, 없으면 브라우저 언어)에 맞는 글
 *   WPW.translate(root)               data-en · data-en-html · data-en-placeholder · data-en-label · data-en-title 을 영어로
 *   WPW.icon('eye-off')               선 아이콘 (조작 화면과 같은 Lucide 한 벌, ISC 라이선스 https://lucide.dev)
 *   WPW.request(url, opts, ms)        fetch + 시간 제한 + JSON. 끊기면 { ok:false, network:true }
 *   WPW.feedback(btn, ok)             누른 단추에 성공(회색 점멸)/실패(주황 흔들림) + 진동
 *   WPW.toast(제목, 설명, {err, ms, key, icon, action:{label, fn}})  화면 위쪽 알림
 *   WPW.wake.start()                  화면 꺼짐 방지 (Wake Lock → 안 되면 소리 없는 작은 영상), WPW.wake.on(cb)
 *   WPW.mobileGuard(onBack)           확대 · 당겨서 새로 고침 · 뒤로 가기 막기 (휴대폰 도구)
 */
(function () {
  'use strict';
  var root = document.documentElement;
  var lang = root.getAttribute('data-app-lang') || (/^ko/i.test(navigator.language || '') ? 'ko' : 'en');
  root.lang = lang;
  root.classList.add('lang-' + lang);
  function t(ko, en) { return lang === 'en' ? en : ko; }
  function esc(s) { return String(s == null ? '' : s).replace(/[&<>"']/g, function (c) { return { '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]; }); }

  function translate(scope) {
    if (lang !== 'en') return;
    var s = scope || document;
    s.querySelectorAll('[data-en]').forEach(function (e) { e.textContent = e.getAttribute('data-en'); });
    s.querySelectorAll('[data-en-html]').forEach(function (e) { e.innerHTML = e.getAttribute('data-en-html'); });
    s.querySelectorAll('[data-en-placeholder]').forEach(function (e) { e.placeholder = e.getAttribute('data-en-placeholder'); });
    s.querySelectorAll('[data-en-label]').forEach(function (e) { e.setAttribute('aria-label', e.getAttribute('data-en-label')); });
    s.querySelectorAll('[data-en-title]').forEach(function (e) { e.title = e.getAttribute('data-en-title'); });
    if (!scope && document.body && document.body.getAttribute('data-en-doc')) document.title = document.body.getAttribute('data-en-doc');
  }

  // ---------- 아이콘 ----------
  var ICONS = {
    'chevron-left': '<path d="m15 18-6-6 6-6"/>',
    'chevron-right': '<path d="m9 18 6-6-6-6"/>',
    'chevron-down': '<path d="m6 9 6 6 6-6"/>',
    'chevron-up': '<path d="m18 15-6-6-6 6"/>',
    'eye': '<path d="M2.062 12.348a1 1 0 0 1 0-.696 10.75 10.75 0 0 1 19.876 0 1 1 0 0 1 0 .696 10.75 10.75 0 0 1-19.876 0"/><circle cx="12" cy="12" r="3"/>',
    'eye-off': '<path d="M10.733 5.076a10.744 10.744 0 0 1 11.205 6.575 1 1 0 0 1 0 .696 10.747 10.747 0 0 1-1.444 2.49"/><path d="M14.084 14.158a3 3 0 0 1-4.242-4.242"/><path d="M17.479 17.499a10.75 10.75 0 0 1-15.417-5.151 1 1 0 0 1 0-.696 10.75 10.75 0 0 1 4.446-5.143"/><path d="m2 2 20 20"/>',
    'square': '<rect width="18" height="18" x="3" y="3" rx="2"/>',
    'list': '<path d="M3 5h.01"/><path d="M3 12h.01"/><path d="M3 19h.01"/><path d="M8 5h13"/><path d="M8 12h13"/><path d="M8 19h13"/>',
    'timer': '<line x1="10" x2="14" y1="2" y2="2"/><line x1="12" x2="15" y1="14" y2="11"/><circle cx="12" cy="14" r="8"/>',
    'layers': '<path d="M12.83 2.18a2 2 0 0 0-1.66 0L2.6 6.08a1 1 0 0 0 0 1.83l8.58 3.91a2 2 0 0 0 1.66 0l8.58-3.9a1 1 0 0 0 0-1.83z"/><path d="M2 12a1 1 0 0 0 .58.91l8.6 3.91a2 2 0 0 0 1.65 0l8.58-3.9A1 1 0 0 0 22 12"/><path d="M2 17a1 1 0 0 0 .58.91l8.6 3.91a2 2 0 0 0 1.65 0l8.58-3.9A1 1 0 0 0 22 17"/>',
    'camera': '<path d="M13.997 4a2 2 0 0 1 1.76 1.05l.486.9A2 2 0 0 0 18.003 7H20a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V9a2 2 0 0 1 2-2h1.997a2 2 0 0 0 1.759-1.048l.489-.904A2 2 0 0 1 10.004 4z"/><circle cx="12" cy="13" r="3"/>',
    'check': '<path d="M20 6 9 17l-5-5"/>',
    'x': '<path d="M18 6 6 18"/><path d="m6 6 12 12"/>',
    'lock': '<rect width="18" height="11" x="3" y="11" rx="2" ry="2"/><path d="M7 11V7a5 5 0 0 1 10 0v4"/>',
    'refresh-cw': '<path d="M3 12a9 9 0 0 1 9-9 9.75 9.75 0 0 1 6.74 2.74L21 8"/><path d="M21 3v5h-5"/><path d="M21 12a9 9 0 0 1-9 9 9.75 9.75 0 0 1-6.74-2.74L3 16"/><path d="M8 16H3v5"/>',
    'plus': '<path d="M5 12h14"/><path d="M12 5v14"/>',
    'trash-2': '<!-- @license lucide-static v0.507.0 - ISC --><path d="M3 6h18"/><path d="M19 6v14c0 1-1 2-2 2H7c-1 0-2-1-2-2V6"/><path d="M8 6V4c0-1 1-2 2-2h4c1 0 2 1 2 2v2"/><line x1="10" x2="10" y1="11" y2="17"/><line x1="14" x2="14" y1="11" y2="17"/>',
    'search': '<path d="m21 21-4.34-4.34"/><circle cx="11" cy="11" r="8"/>',
    'music': '<!-- @license lucide-static v0.507.0 - ISC --><path d="M9 18V5l12-2v13"/><circle cx="6" cy="18" r="3"/><circle cx="18" cy="16" r="3"/>',
    'music-2': '<!-- @license lucide-static v0.507.0 - ISC --><circle cx="8" cy="18" r="4"/><path d="M12 18V2l7 4"/>',
    'book-open': '<path d="M12 5v16"/><path d="M20.001 19A2 2 0 0022 17V5a2 2 0 00-1.999-2L16 3.002A5 5 0 0012 5a5 5 0 00-4-2H4a2 2 0 00-2 2v12a2 2 0 001.999 2H8a5 5 0 014 2 5 5 0 014-2z"/>',
    'megaphone': '<path d="M11 6a13 13 0 0 0 8.4-2.8A1 1 0 0 1 21 4v12a1 1 0 0 1-1.6.8A13 13 0 0 0 11 14H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2z"/><path d="M6 14a12 12 0 0 0 2.4 7.2 2 2 0 0 0 3.2-2.4A8 8 0 0 1 10 14"/><path d="M8 6v8"/>',
    'tag': '<path d="M12.586 2.586A2 2 0 0 0 11.172 2H4a2 2 0 0 0-2 2v7.172a2 2 0 0 0 .586 1.414l8.704 8.704a2.426 2.426 0 0 0 3.42 0l6.58-6.58a2.426 2.426 0 0 0 0-3.42z"/><circle cx="7.5" cy="7.5" r=".5" fill="currentColor"/>',
    'image': '<rect width="18" height="18" x="3" y="3" rx="2" ry="2"/><circle cx="9" cy="9" r="2"/><path d="m21 15-3.086-3.086a2 2 0 0 0-2.828 0L6 21"/>',
    'clapperboard': '<path d="m12.296 3.464 3.02 3.956"/><path d="M20.2 6 3 11l-.9-2.4c-.3-1.1.3-2.2 1.3-2.5l13.5-4c1.1-.3 2.2.3 2.5 1.3z"/><path d="M3 11h18v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"/><path d="m6.18 5.276 3.1 3.899"/>',
    'volume-2': '<path d="M11 4.702a.705.705 0 0 0-1.203-.498L6.413 7.587A1.4 1.4 0 0 1 5.416 8H3a1 1 0 0 0-1 1v6a1 1 0 0 0 1 1h2.416a1.4 1.4 0 0 1 .997.413l3.383 3.384A.705.705 0 0 0 11 19.298z"/><path d="M16 9a5 5 0 0 1 0 6"/><path d="M19.364 18.364a9 9 0 0 0 0-12.728"/>',
    'images': '<path d="m22 11-1.296-1.296a2.4 2.4 0 0 0-3.408 0L11 16"/><path d="M4 8a2 2 0 0 0-2 2v10a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2"/><circle cx="13" cy="7" r="1" fill="currentColor"/><rect x="8" y="2" width="14" height="14" rx="2"/>',
    'globe': '<circle cx="12" cy="12" r="10"/><path d="M12 2a14.5 14.5 0 0 0 0 20 14.5 14.5 0 0 0 0-20"/><path d="M2 12h20"/>',
    'presentation': '<path d="M2 3h20"/><path d="M21 3v11a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V3"/><path d="m7 21 5-5 5 5"/>',
    'clock': '<circle cx="12" cy="12" r="10"/><path d="M12 6v6l4 2"/>',
    'pause': '<rect x="14" y="3" width="5" height="18" rx="1"/><rect x="5" y="3" width="5" height="18" rx="1"/>',
    'play': '<path d="M5 5a2 2 0 0 1 3.008-1.728l11.997 6.998a2 2 0 0 1 .003 3.458l-12 7A2 2 0 0 1 5 19z"/>',
    'rotate-ccw': '<path d="M3 12a9 9 0 1 0 9-9 9.75 9.75 0 0 0-6.74 2.74L3 8"/><path d="M3 3v5h5"/>',
    'smartphone': '<rect width="14" height="20" x="5" y="2" rx="2" ry="2"/><path d="M12 18h.01"/>',
    'monitor': '<rect width="20" height="14" x="2" y="3" rx="2"/><line x1="8" x2="16" y1="21" y2="21"/><line x1="12" x2="12" y1="17" y2="21"/>',
    'arrow-up': '<path d="m5 12 7-7 7 7"/><path d="M12 19V5"/>',
    'arrow-down': '<path d="M12 5v14"/><path d="m19 12-7 7-7-7"/>',
    'info': '<circle cx="12" cy="12" r="10"/><path d="M12 16v-4"/><path d="M12 8h.01"/>',
    'circle-help': '<!-- @license lucide-static v0.507.0 - ISC --><circle cx="12" cy="12" r="10"/><path d="M9.09 9a3 3 0 0 1 5.83 1c0 2-3 3-3 3"/><path d="M12 17h.01"/>',
    'undo-2': '<path d="M9 14 4 9l5-5"/><path d="M4 9h10.5a5.5 5.5 0 0 1 5.5 5.5a5.5 5.5 0 0 1-5.5 5.5H11"/>',
    'skip-back': '<path d="M17.971 4.285A2 2 0 0 1 21 6v12a2 2 0 0 1-3.029 1.715l-9.997-5.998a2 2 0 0 1-.003-3.432z"/><path d="M3 20V4"/>',
    'skip-forward': '<path d="M21 4v16"/><path d="M6.029 4.285A2 2 0 0 0 3 6v12a2 2 0 0 0 3.029 1.715l9.997-5.998a2 2 0 0 0 .003-3.432z"/>',
    'pencil': '<path d="M21.174 6.812a1 1 0 0 0-3.986-3.987L3.842 16.174a2 2 0 0 0-.5.83l-1.321 4.352a.5.5 0 0 0 .623.622l4.353-1.32a2 2 0 0 0 .83-.497z"/><path d="m15 5 4 4"/>',
    'file-plus': '<path d="M6 22a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h8a2.4 2.4 0 0 1 1.704.706l3.588 3.588A2.4 2.4 0 0 1 20 8v12a2 2 0 0 1-2 2z"/><path d="M14 2v5a1 1 0 0 0 1 1h5"/><path d="M9 15h6"/><path d="M12 18v-6"/>',
    'ellipsis': '<circle cx="12" cy="12" r="1"/><circle cx="19" cy="12" r="1"/><circle cx="5" cy="12" r="1"/>',
    'wifi': '<path d="M12 20h.01"/><path d="M2 8.82a15 15 0 0 1 20 0"/><path d="M5 12.859a10 10 0 0 1 14 0"/><path d="M8.5 16.429a5 5 0 0 1 7 0"/>',
    'wifi-off': '<path d="M12 20h.01"/><path d="M8.5 16.429a5 5 0 0 1 7 0"/><path d="M5 12.859a10 10 0 0 1 5.17-2.69"/><path d="M19 12.859a10 10 0 0 0-2.007-1.523"/><path d="M2 8.82a15 15 0 0 1 4.177-2.643"/><path d="M22 8.82a15 15 0 0 0-11.288-3.764"/><path d="m2 2 20 20"/>',
    'power': '<path d="M12 2v10"/><path d="M18.4 6.6a9 9 0 1 1-12.77.04"/>',
    'user': '<path d="M19 21v-2a4 4 0 0 0-4-4H9a4 4 0 0 0-4 4v2"/><circle cx="12" cy="7" r="4"/>',
    'moon': '<path d="M12 3a6 6 0 0 0 9 9 9 9 0 1 1-9-9Z"/>',
    'save': '<path d="M15.2 3a2 2 0 0 1 1.4.6l3.8 3.8a2 2 0 0 1 .6 1.4V19a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2z"/><path d="M17 21v-7a1 1 0 0 0-1-1H8a1 1 0 0 0-1 1v7"/><path d="M7 3v4a1 1 0 0 0 1 1h7"/>',
    'grip-vertical': '<circle cx="9" cy="12" r="1"/><circle cx="9" cy="5" r="1"/><circle cx="9" cy="19" r="1"/><circle cx="15" cy="12" r="1"/><circle cx="15" cy="5" r="1"/><circle cx="15" cy="19" r="1"/>',
    'switch-camera': '<path d="M11 19H4a2 2 0 0 1-2-2V7a2 2 0 0 1 2-2h5"/><path d="M13 5h7a2 2 0 0 1 2 2v10a2 2 0 0 1-2 2h-5"/><circle cx="12" cy="12" r="3"/><path d="m18 22-3-3 3-3"/><path d="m6 2 3 3-3 3"/>',
    'battery': '<rect width="16" height="10" x="2" y="7" rx="2" ry="2"/><line x1="22" x2="22" y1="11" y2="13"/>',
    'message-square': '<path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z"/>',
    'vibrate': '<path d="m2 8 2 2-2 2 2 2-2 2"/><path d="m22 8-2 2 2 2-2 2 2 2"/><rect width="8" height="14" x="8" y="5" rx="1"/>',
    'log-out': '<path d="M9 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h4"/><path d="m16 17 5-5-5-5"/><path d="M21 12H9"/>',
    'circle-alert': '<circle cx="12" cy="12" r="10"/><line x1="12" x2="12" y1="8" y2="12"/><line x1="12" x2="12.01" y1="16" y2="16"/>'
  };
  var TYPE = { hymn: 'music-2', bible: 'book-open', song: 'music', text: 'megaphone', title: 'tag', image: 'image', video: 'clapperboard', audio: 'volume-2', slideshow: 'images', camera: 'camera', blank: 'square', weblyrics: 'globe', ppt: 'presentation', score: 'music-2' };
  function icon(name, cls) {
    var p = ICONS[name];
    if (!p) return '';
    return '<svg class="ic' + (cls ? ' ' + cls : '') + '" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">' + p + '</svg>';
  }
  function typeIcon(type, cls) { return icon(TYPE[type] || 'square', cls); }
  /** 정적 HTML 의 <i data-icon="…"> 를 svg 로 */
  function applyIcons(scope) {
    (scope || document).querySelectorAll('i[data-icon]').forEach(function (el) { var h = icon(el.getAttribute('data-icon'), el.className); if (h) el.outerHTML = h; });
  }

  // ---------- 저장 (사생활 보호 창 · 막힌 저장소에서도 죽지 않게) ----------
  function store(kind, key, val) {
    try {
      var s = window[kind];
      if (val === undefined) return s.getItem(key) || '';
      if (val === null) s.removeItem(key); else s.setItem(key, String(val));
    } catch (_) { /* 저장 안 됨 */ }
    return '';
  }

  // ---------- 요청 ----------
  function request(url, options, ms) {
    var ctl = typeof AbortController === 'function' ? new AbortController() : null;
    var timer = setTimeout(function () { if (ctl) ctl.abort(); }, ms || 8000);
    var opt = Object.assign({ cache: 'no-store' }, options || {});
    if (ctl) opt.signal = ctl.signal;
    return fetch(url, opt).then(function (r) {
      return r.json().catch(function () { return { ok: false }; }).then(function (j) {
        j = j && typeof j === 'object' ? j : { ok: false };
        j.status = r.status;
        if (!r.ok && j.ok == null) j.ok = false;
        return j;
      });
    }).catch(function (e) {
      return { ok: false, network: true, timeout: !!(e && e.name === 'AbortError'), status: 0 };
    }).then(function (j) { clearTimeout(timer); return j; });
  }

  // ---------- 누름 반응 · 진동 ----------
  var vib = store('localStorage', 'wp-vibrate') !== '0';
  function buzz(kind) {
    if (!vib || !navigator.vibrate) return;
    try { navigator.vibrate(kind === 'err' ? [40, 50, 40] : kind === 'tap' ? 8 : 14); } catch (_) { /* 무시 */ }
  }
  function feedback(b, ok) {
    if (b) {
      b.classList.remove('did-ok', 'did-err');
      void b.offsetWidth; // 같은 효과를 다시 틀기 위해
      b.classList.add(ok ? 'did-ok' : 'did-err');
      setTimeout(function () { b.classList.remove('did-ok', 'did-err'); }, 700);
    }
    buzz(ok ? 'ok' : 'err');
  }
  // 손가락이 닿는 순간 눌림 표시 (iOS 사파리는 :active 가 늦거나 안 생긴다). 떼거나 밀려나면 끈다.
  function unpress() { document.querySelectorAll('.pressing').forEach(function (e) { e.classList.remove('pressing'); }); }
  document.addEventListener('pointerdown', function (e) {
    var b = e.target && e.target.closest && e.target.closest('button, .tap');
    if (b && !b.disabled) b.classList.add('pressing');
  }, { passive: true, capture: true });
  ['pointerup', 'pointercancel', 'scroll', 'blur'].forEach(function (ev) { window.addEventListener(ev, unpress, { passive: true, capture: true }); });
  document.addEventListener('touchstart', function () {}, { passive: true }); // iOS: :active 켜기

  // ---------- 알림 (화면 위쪽 — 엄지가 닿는 아래 단추를 가리지 않게) ----------
  var toastBox = null;
  function toast(title, detail, o) {
    o = o || {};
    if (!toastBox) {
      toastBox = document.createElement('div');
      toastBox.className = 'toasts';
      toastBox.setAttribute('role', 'status');
      toastBox.setAttribute('aria-live', 'polite');
      document.body.appendChild(toastBox);
    }
    if (o.key) { var same = toastBox.querySelector('[data-key="' + o.key + '"]'); if (same) same.remove(); }
    while (toastBox.children.length >= 2) toastBox.firstChild.remove();
    var el = document.createElement('div');
    el.className = 'toast' + (o.err ? ' err' : '');
    if (o.key) el.setAttribute('data-key', o.key);
    el.innerHTML = icon(o.err ? 'circle-alert' : (o.icon || 'check'), 'lead') + '<div><b>' + esc(title) + '</b>' + (detail ? '<span>' + esc(detail) + '</span>' : '') + '</div>' + (o.action ? '<button class="btn flat act">' + esc(o.action.label) + '</button>' : '') + '<button class="x" aria-label="' + t('닫기', 'Close') + '">' + icon('x') + '</button>';
    var close = function () { if (el.parentNode) el.remove(); };
    el.querySelector('.x').onclick = close;
    if (o.action) el.querySelector('.act').onclick = function () { close(); o.action.fn(); };
    toastBox.appendChild(el);
    var ms = o.ms != null ? o.ms : (o.err ? 6000 : 2200);
    if (ms > 0) setTimeout(close, ms);
    return { close: close, el: el };
  }

  // ---------- 화면 꺼짐 방지 ----------
  // 1) Wake Lock — https 에서만 (휴대폰 카메라 화면). 리모컨은 내부망 http 라 대개 이 길이 없다.
  // 2) 안 되면 소리 없는 작은(32×32) 영상을 되풀이 재생 (아이폰 · 안드로이드는 영상 재생 중 화면을 끄지 않는다).
  //    영상 재생은 사용자 동작 안에서만 허락되므로 화면을 처음 누를 때 시작한다. (실기 확인 필요 — docs/web-craft.md)
  var AWAKE_MP4 = 'data:video/mp4;base64,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';
  var wake = (function () {
    var want = false, lock = null, video = null, state = 'off', cbs = [];
    function set(s) { if (state !== s) { state = s; cbs.forEach(function (f) { try { f(s); } catch (_) { /* 무시 */ } }); } }
    function viaVideo() {
      if (!video) {
        video = document.createElement('video');
        video.setAttribute('playsinline', '');
        video.setAttribute('muted', '');
        video.muted = true;
        video.loop = true;
        video.src = AWAKE_MP4;
        video.setAttribute('aria-hidden', 'true');
        video.style.cssText = 'position:fixed;left:0;top:0;width:2px;height:2px;opacity:.01;pointer-events:none;z-index:-1';
        document.body.appendChild(video);
      }
      var p = video.play();
      if (p && p.then) p.then(function () { set('video'); }, function () { set('need-tap'); });
      else set('video');
    }
    function apply() {
      if (!want || document.visibilityState !== 'visible' || lock) return;
      if (navigator.wakeLock && window.isSecureContext) {
        navigator.wakeLock.request('screen').then(function (l) {
          lock = l;
          set('on');
          l.addEventListener('release', function () { lock = null; if (want) set(document.visibilityState === 'visible' ? 'need-tap' : 'off'); });
        }, function () { viaVideo(); });
      } else viaVideo();
    }
    document.addEventListener('visibilitychange', function () { if (document.visibilityState === 'visible') apply(); });
    document.addEventListener('pointerdown', function () { if (want && state !== 'on' && state !== 'video') apply(); }, { passive: true });
    return {
      start: function () { want = true; apply(); },
      on: function (f) { cbs.push(f); f(state); },
      state: function () { return state; },
      text: function (s) {
        s = s || state;
        return s === 'on' || s === 'video' ? t('화면 꺼짐 방지 켜짐', 'Screen stays on')
          : s === 'need-tap' ? t('화면을 한 번 누르면 꺼짐 방지가 켜집니다', 'Tap the screen once to keep it awake')
          : t('화면 꺼짐 방지 꺼짐', 'Screen may turn off');
      }
    };
  })();

  // ---------- 휴대폰 도구: 확대 · 당겨서 새로 고침 · 뒤로 가기 막기 ----------
  function mobileGuard(onBack) {
    root.classList.add('tool');
    // 두 손가락 확대 (iOS 는 user-scalable=no 를 무시한다)
    document.addEventListener('gesturestart', function (e) { e.preventDefault(); }, { passive: false });
    document.addEventListener('touchmove', function (e) { if (e.touches && e.touches.length > 1) e.preventDefault(); }, { passive: false });
    // 두 번 눌러 확대는 CSS touch-action: manipulation 으로만 막는다.
    // (touchend 를 막는 방식은 0.3초 안에 연달아 누른 '다음 다음' 의 두 번째를 삼켜서 쓰지 않는다)
    // 뒤로 가기: 기록을 하나 쌓아 두고, 돌아오면 다시 쌓는다 (안드로이드 뒤로 몸짓 · 아이폰 가장자리 쓸기)
    try {
      history.pushState({ wp: 1 }, '', location.href);
      window.addEventListener('popstate', function () {
        history.pushState({ wp: 1 }, '', location.href);
        if (onBack) onBack();
      });
    } catch (_) { /* 무시 */ }
  }

  // ---------- 화면 높이 · 안전 영역 ----------
  function size() {
    if (!(window.CSS && CSS.supports && CSS.supports('height', '100dvh'))) root.style.setProperty('--vh', window.innerHeight + 'px');
  }
  window.addEventListener('resize', size);
  window.addEventListener('orientationchange', function () { setTimeout(size, 250); });
  size();
  // 시험용: ?safe=위,오른쪽,아래,왼쪽 → 노치 · 홈 막대 흉내 (실제 기기는 env(safe-area-inset-*) 가 준다)
  var safe = new URLSearchParams(location.search).get('safe');
  if (safe && /^\d+,\d+,\d+,\d+$/.test(safe)) {
    safe.split(',').forEach(function (v, i) { root.style.setProperty(['--sat', '--sar', '--sab', '--sal'][i], Math.min(100, +v) + 'px'); });
  }

  // ---------- 어둡게 (어두운 예배당) ----------
  function dim(on) {
    if (on === undefined) return root.classList.contains('dim');
    root.classList.toggle('dim', !!on);
    store('localStorage', 'wp-dim', on ? '1' : null);
    return !!on;
  }
  if (store('localStorage', 'wp-dim') === '1') root.classList.add('dim');

  window.WPW = {
    lang: lang, t: t, esc: esc, translate: translate,
    icon: icon, typeIcon: typeIcon, applyIcons: applyIcons,
    local: function (k, v) { return store('localStorage', k, v); },
    session: function (k, v) { return store('sessionStorage', k, v); },
    request: request, feedback: feedback, buzz: buzz, toast: toast,
    vibrate: function (on) { if (on === undefined) return vib; vib = !!on; store('localStorage', 'wp-vibrate', on ? null : '0'); if (on) buzz('ok'); return vib; },
    canVibrate: !!navigator.vibrate,
    wake: wake, mobileGuard: mobileGuard, dim: dim
  };
  // 스크립트는 body 끝에서 부르므로 정적 아이콘 · 영어는 바로 바꾼다
  applyIcons();
  translate();
})();
