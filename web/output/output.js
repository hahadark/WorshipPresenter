/* 출력 화면 (전체 화면 창 / OBS 브라우저 소스 공용)
 * - SSE 로 Live 상태를 받아 그린다. 연결이 끊겨도 마지막 화면을 그대로 유지하고,
 *   재연결되면 서버가 최신 상태를 다시 보내준다.
 * - 전환: 디졸브(새 층이 이전 층 위로 서서히 나타난 뒤 이전 층을 지움) / 컷(바로 바꿈)
 */
(function () {
  'use strict';
  const R = window.BCRender;
  // 출력 이름 = 주소 끝 (/outputs/main · /outputs/overlay · 추가 출력 /outputs/out-…, 로드맵 3단계). 모르는 것은 예전처럼 예배당
  const tail = location.pathname.replace(/\/+$/, '').split('/').pop() || '';
  const output = tail === 'overlay' ? 'overlay' : /^out-[A-Za-z0-9_-]{1,36}$/.test(tail) ? tail : 'main';
  const params = new URLSearchParams(location.search);
  const isWindow = params.has('window');
  const screenEl = document.getElementById('screen');

  let clientId = null;
  let stage = null;
  let tplKey = '';
  let tpl = null;
  let current = null; // 현재 보이는 본문층
  let lastKey = '';
  let lastPayload = null;
  let drawState = null; // 화면에 그리기·강조

  // ---------- 소스마다 층 하나 (docs/roadmap-sources.md) ----------
  // 무대(.bc-stage) 바로 아래에는 층 상자(.wp-layer)만 둔다. 화면 구성(payload.scene.layers)의 차례가 곧 쌓는 차례이고,
  // 배경 · 미디어 · 본문 · 오버레이 하나하나 · 로고 화면 · 그리기가 자기 층 안에만 그린다.
  // 층 상자는 z-index · opacity · transform 을 주지 않아(칸 값이 없을 때) 쌓임 맥락을 새로 만들지 않는다 → 안의 z 10 · 20 · 50 이 전과 같다.
  const slots = new Map(); // 칸 id → 층 상자
  let sceneSources = {};
  /** 옛 코어(scene 없음)와 같은 기본 차례 */
  function defaultLayers(p) {
    const l = id => ({ id, source: id });
    const ov = (p.overlays || []).filter(o => o.id !== 'wpmessage').map(o => l(o.id === 'wpcaption' ? 'caption' : 'overlay:' + o.id));
    return [l('background'), l('media'), l('slide-bg'), l('live'), ...ov, l('message'), l('logo-screen'), l('draw')];
  }
  function sceneLayers(p) {
    const s = p && p.scene;
    return (s && Array.isArray(s.layers) ? s.layers : defaultLayers(p || {})).filter(l => l && l.id && l.source !== 'pip'); // pip 는 무대 밖 (#bcPip)
  }
  /** 칸 상자 모양 (render.js sceneBoxCss): 기본 구성의 칸은 'position:absolute;inset:0' 뿐 → 쌓임 맥락을 새로 만들지 않는다 */
  function layerBox(el, l) {
    const s = R.sceneBoxCss(l);
    if (el.style.cssText !== s) el.style.cssText = s;
  }
  /** 그 자리에서 옮겨 갈 수 있는 소스 (안의 그림이 템플릿 · 칸 크기와 상관없음): 카메라 · 사용자 소스(웹 페이지).
   *  이 칸은 늘 그릴 자리(host)를 두어, 자리 · 자르기가 바뀌어도 같은 video · iframe 요소를 그대로 쓴다 (로드맵 4단계) */
  function movable(l, p) {
    const src = l.source || '';
    return src.startsWith('camera:') || !!((p && p.sources) || sceneSources)[src];
  }
  /** 상자 안 그릴 자리를 다시 만들어야 하는 값 (자리 · 자르기 · 글 모양 · 둥글기 · 테두리 · 무대 크기) */
  function hostKey(l, mv) {
    if (!R.sceneNeedsBox(l) && !mv) return '';
    const t = l.template && typeof l.template === 'object' ? l.template : null;
    return JSON.stringify([l.rect, l.crop, l.radius, l.border, t ? JSON.stringify(t) : '', tpl && tpl.width, tpl && tpl.height]);
  }
  // ---------- 구성이 바뀔 때의 전환 (로드맵 4단계, 코어 payload.sceneTransition) ----------
  // move(옮겨 가기, 기본): 같은 소스는 같은 요소 그대로 새 자리로 미끄러지고, 새로 생기거나 사라지는 칸은 서서히.
  // fade(서서히): 바뀐 칸은 모두 서서히 (카메라도 같은 스트림으로 겹쳐 바뀜). cut(바로).
  // 처음 그릴 때 · 같은 구성 안에서 칸을 고칠 때(구성 편집기)는 전환 없이 바로 — 2.0.8 과 같은 그림.
  let sceneId = null;
  const opacityOf = l => (l && l.opacity != null && Number(l.opacity) < 1 ? Math.max(0, Number(l.opacity) || 0) : 1);
  function enter(el, l, ms) {
    if (!ms || !el.animate) return;
    el.animate([{ opacity: 0 }, { opacity: opacityOf(l) }], { duration: ms, easing: 'ease' });
  }
  function leave(el, ms) {
    el._leaving = true;
    if (!ms || !el.animate) { el.remove(); return; }
    const a = el.animate([{ opacity: opacityOf(el._layer) }, { opacity: 0 }], { duration: ms, easing: 'ease', fill: 'forwards' });
    a.onfinish = () => el.remove();
    setTimeout(() => el.remove(), ms + 200);
  }
  /** 옮겨 가기: 새 자리에 놓은 상자를 예전 자리 모양에서 시작해 새 자리로 (transform 만 — 안의 요소는 그대로) */
  function glide(el, from, to, ms) {
    if (!ms || !el.animate || !tpl) return;
    const a = R.sceneRect(from), b = R.sceneRect(to);
    if (a.every((x, i) => Math.abs(x - b[i]) < 1e-6)) return;
    const sx = a[2] / Math.max(1e-4, b[2]), sy = a[3] / Math.max(1e-4, b[3]);
    const tx = (a[0] - b[0]) * tpl.width, ty = (a[1] - b[1]) * tpl.height;
    el.animate([
      { transformOrigin: '0 0', transform: `translate(${tx}px,${ty}px) scale(${sx},${sy})`, opacity: opacityOf(from) },
      { transformOrigin: '0 0', transform: 'none', opacity: opacityOf(to) },
    ], { duration: ms, easing: 'ease-in-out' });
  }
  /** 서서히 바꿀 때 예전 자리의 카메라를 잠깐 남겨 둔다 (같은 스트림 — 다시 열지 않음) */
  function camGhost(el, l, ms) {
    if (!ms || !String(l.source).startsWith('camera:')) return;
    const g = document.createElement('div');
    g.className = 'wp-layer';
    g.dataset.ghost = '1';
    g.style.cssText = R.sceneBoxCss(l);
    const h = R.sceneHost(g, l, tpl).host;
    const m = R.buildMediaLayer({ type: 'camera', deviceId: String(l.source).slice(7), fit: l.fit === 'contain' ? 'contain' : 'cover' });
    m.className = 'wp-cam';
    m.style.cssText = 'position:absolute;inset:0;overflow:hidden';
    h.appendChild(m);
    g._layer = l;
    stage.insertBefore(g, el);
    leave(g, ms);
  }
  /** 구성 차례대로 층 상자를 맞춘다. 이미 있는 상자는 차례가 맞으면 옮기지 않는다 (옮기면 웹 페이지가 다시 열린다).
   *  짝 맞추기: 같은 칸 id + 같은 소스 → 없으면 같은 소스 (구성이 바뀌어도 같은 카메라 · 웹 페이지 요소를 그대로).
   *  자리 · 자르기 · 글 모양이 바뀐 칸: 옮겨 갈 수 있는 소스는 그 상자를 새 자리에 맞추고, 나머지는 상자를 새로 만들어 그 소스를 다시 그린다
   *  (돌려줌: 새로 만든 칸의 소스들) */
  function syncLayers(p) {
    const want = sceneLayers(p);
    const sid = p && p.scene ? String(p.scene.id || '') : '';
    const changed = !!sceneId && !!sid && sid !== sceneId; // 처음 그릴 때(이전 구성 없음)는 전환 없이
    sceneId = sid;
    if (sid) document.body.dataset.scene = sid; // 점검용 (그림에는 안 씀)
    const tr = (changed && p && p.sceneTransition) || { type: 'cut', ms: 0 };
    const type = ['move', 'fade', 'cut'].includes(tr.type) ? tr.type : 'cut';
    const ms = type === 'cut' ? 0 : Math.max(0, Math.min(3000, Number(tr.ms) || 0));
    const fresh = new Set();
    const pool = new Map(slots);
    const match = new Map();
    for (const l of want) { const el = pool.get(l.id); if (el && el.dataset.source === l.source) { match.set(l.id, el); pool.delete(l.id); } }
    for (const l of want) {
      if (match.has(l.id)) continue;
      for (const [id, el] of pool) if (el.dataset.source === l.source) { match.set(l.id, el); pool.delete(id); break; }
    }
    for (const el of pool.values()) leave(el, ms); // 사라지는 칸
    slots.clear();
    let prev = null;
    for (const l of want) {
      let el = match.get(l.id);
      const mv = movable(l, p);
      const hk = hostKey(l, mv);
      if (el && (el.dataset.hostKey || '') !== hk) {
        if (mv && el._host) {
          // 같은 요소를 새 자리로: 상자 · 그릴 자리만 고친다
          const from = el._layer || l;
          el._tpl = R.sceneHostFit(el, el._host, l, tpl);
          el.dataset.hostKey = hk;
          el.style.cssText = R.sceneBoxCss(l);
          if (type === 'move') glide(el, from, l, ms);
          else if (type === 'fade') { camGhost(el, from, ms); enter(el, l, ms); }
        } else { leave(el, ms); el = null; }
      }
      if (!el) {
        el = document.createElement('div');
        el.className = 'wp-layer';
        el.dataset.source = l.source;
        el.dataset.hostKey = hk;
        if (hk) { const h = R.sceneHost(el, l, tpl); el._host = h.host; el._tpl = h.t; }
        fresh.add(l.source);
        el.style.cssText = R.sceneBoxCss(l);
        enter(el, l, ms);
      }
      el.dataset.layer = l.id;
      slots.set(l.id, el);
      let at = prev ? prev.nextSibling : stage.firstChild;
      while (at && (at._leaving || at.dataset.ghost)) at = at.nextSibling;
      if (el !== at) stage.insertBefore(el, at);
      el._layer = l;
      layerBox(el, l);
      prev = el;
    }
    return fresh;
  }
  /** 이 소스를 그릴 자리 (구성에 없으면 null — 그 소스는 이 화면에 그리지 않는다). 상자가 있는 칸이면 상자 안 그릴 자리 */
  function slotEl(source) {
    for (const el of slots.values()) if (el.dataset.source === source) return el;
    return null;
  }
  function slot(source) {
    const el = slotEl(source);
    return el ? (el._host || el) : null;
  }
  /** 이 소스를 그릴 때 쓰는 템플릿 (칸 크기에 맞춘 것, 없으면 무대 템플릿) */
  function slotTpl(source) {
    const el = slotEl(source);
    return (el && el._tpl) || tpl;
  }
  /** 템플릿 배경을 'background' 칸에: 무대에 바로면 무대의 배경, 상자 칸이면 그 칸 크기 템플릿으로 새로 */
  let stageBg = null;
  function placeBackground() {
    const el = slotEl('background');
    if (!el) { if (stageBg) stageBg.remove(); return; }
    const host = el._host || el;
    if (host.querySelector(':scope > .bc-bg')) return;
    if (stageBg && stageBg.isConnected && !el.contains(stageBg)) stageBg = R.buildStage(tpl).firstChild; // 예전 배경은 사라지는 칸과 함께 서서히
    const bg = el._host ? R.buildStage(el._tpl).firstChild : stageBg;
    if (bg) host.insertBefore(bg, host.firstChild);
  }
  /** 카메라 칸 (camera:<deviceId>): 칸마다 영상 하나 — 같은 카메라는 페이지 안에서 스트림 하나를 같이 쓴다 (render.js cameraStream) */
  function renderCams() {
    for (const el of slots.values()) {
      const src = el.dataset.source || '';
      if (!src.startsWith('camera:')) continue;
      const l = el._layer || {};
      const key = JSON.stringify([src, l.fit]);
      const host = el._host || el;
      if (host.dataset.camKey === key && host.querySelector('.wp-cam')) continue;
      host.dataset.camKey = key;
      host.querySelectorAll('.wp-cam').forEach(x => x.remove());
      const m = R.buildMediaLayer({ type: 'camera', deviceId: src.slice(7), fit: l.fit === 'contain' ? 'contain' : 'cover' });
      m.className = 'wp-cam'; // 배경 끄기(.hide-bg .bc-medialayer)에 숨지 않게 — 구성에 직접 넣은 카메라
      m.style.cssText = 'position:absolute;inset:0;overflow:hidden';
      host.appendChild(m);
    }
  }

  // ---------- 사용자 소스: 웹 페이지 · 화면 캡처 ----------
  function renderUserSources(p) {
    sceneSources = p.sources || {};
    for (let el of slots.values()) {
      const id = el.dataset.source;
      const s = sceneSources[id];
      if (!s) continue;
      const box = el;
      const hostEl = box._host || box;
      const fit = (box._layer && box._layer.fit) || s.fit;
      const key = JSON.stringify([s.kind, s.url, s.target, fit]);
      if (hostEl.dataset.key === key) { hostEl.firstChild && (hostEl.firstChild.style.opacity = String(s.opacity ?? 1)); continue; }
      hostEl.dataset.key = key;
      hostEl.innerHTML = '';
      el = hostEl;
      if (s.kind === 'web' && /^https?:\/\//i.test(s.url || '')) {
        // 다른 사이트가 출력 화면을 다른 곳으로 옮기지 못하게 sandbox (top 이동 · 창 띄우기 막음). 누르기는 받지 않는다
        const f = document.createElement('iframe');
        f.className = 'wp-web';
        f.src = s.url;
        f.setAttribute('sandbox', 'allow-scripts allow-same-origin allow-forms');
        f.setAttribute('allow', 'autoplay');
        f.setAttribute('referrerpolicy', 'no-referrer');
        f.style.cssText = `position:absolute;inset:0;width:100%;height:100%;border:0;background:transparent;pointer-events:none;opacity:${s.opacity ?? 1}`;
        el.appendChild(f);
      }
      // 화면 캡처(kind capture)는 출력 창에서 허락 창 없이 열 방법이 없어 아직 그리지 않는다 (docs/roadmap-sources.md (마))
    }
  }

  function setupStage(t) {
    const key = JSON.stringify(t);
    if (key === tplKey && stage) return false;
    tplKey = key;
    tpl = t;
    // 템플릿이 바뀌어도 카메라 · 웹 페이지 칸(옮겨 갈 수 있는 소스)은 같은 요소를 새 무대로 옮긴다
    // (로드맵 4단계 — 순서가 바뀌며 템플릿이 달라져도 카메라가 끊기지 않게). hostKey '-' = 새 무대 크기에 다시 맞춤
    const keep = [...slots].filter(([, el]) => el._host && movable(el._layer || { source: el.dataset.source }, lastPayload));
    vUrl = ''; vLayer = null;
    stage = R.buildStage(t);
    stage.style.setProperty('--bc-fade', fadeMs() + 'ms');
    // 템플릿 배경(.bc-bg)은 'background' 층으로 옮겨 둔다
    stageBg = stage.firstChild;
    stageBg.remove();
    slots.clear();
    for (const [id, el] of keep) { el.dataset.hostKey = '-'; slots.set(id, el); stage.appendChild(el); }
    screenEl.innerHTML = '';
    syncLayers(lastPayload);
    placeBackground();
    screenEl.appendChild(stage);
    current = null;
    // 투명 템플릿이 아니고 OBS 가 아닌 창이면 여백(레터박스)을 검게
    document.body.classList.toggle('opaque', t.background.type !== 'transparent' && isWindow);
    rescale();
    return true;
  }

  function rescale() {
    if (stage && tpl) R.scaleStage(stage, screenEl, tpl);
  }
  window.addEventListener('resize', rescale);

  function fadeMs() { return tpl && tpl.animation && ['dissolve', 'fade'].includes(tpl.animation.type) ? Math.max(0, Number(tpl.animation.duration) || 0) : 0; }

  let renderSeq = 0;

  // ---------- 순서 배경 영상 층 ----------
  let vUrl = '';
  let vLayer = null;
  function setVideoBg(url) {
    if (url === vUrl) return;
    vUrl = url;
    const old = vLayer;
    vLayer = null;
    const ms = Math.max(300, fadeMs());
    if (old) { old.style.transition = `opacity ${ms}ms ease`; old.style.opacity = '0'; setTimeout(() => old.remove(), ms + 60); }
    if (!url) return;
    const layer = document.createElement('div');
    layer.className = 'bc-vlayer';
    layer.style.opacity = '0';
    layer.innerHTML = `<video src="${R.safeUrl(url)}" autoplay muted loop playsinline preload="auto"></video>`;
    const sl = slot('slide-bg'); // 템플릿 배경 · 배경 미디어 위, 글자 아래 (구성 차례)
    if (!sl) return;
    sl.appendChild(layer);
    const v = layer.querySelector('video');
    const show = () => { layer.style.transition = `opacity ${ms}ms ease`; layer.style.opacity = '1'; };
    if (v.readyState >= 2) requestAnimationFrame(show); else { v.addEventListener('loadeddata', show, { once: true }); setTimeout(show, 1500); }
    vLayer = layer;
  }

  function fadeIn(el, ms, delay) {
    requestAnimationFrame(() => requestAnimationFrame(() => {
      el.style.transition = `opacity ${ms}ms ease ${delay}ms`;
      el.style.opacity = '1';
    }));
  }

  function fadeOut(els, ms, delay) {
    for (const el of els) {
      if (!ms) { el.remove(); continue; }
      el.style.transition = `opacity ${ms}ms ease ${delay}ms`;
      el.style.opacity = '0';
      setTimeout(() => el.remove(), ms + delay + 50);
    }
  }

  /** 이 슬라이드가 화면을 꽉 채우는(불투명한) 그림인가 */
  function coversScreen(slide, t) {
    if (R.lyricsOnly(t || tpl) && typeof slide.caption === 'string') return false; // 가사만 출력
    if (slide.kind === 'score') return !!slide.bg;
    if (slide.kind === 'image' || slide.kind === 'video') return true;
    if (slide.kind === 'blank') return slide.mode !== 'background';
    return false;
  }

  /** 층 안의 그림과 슬라이드 배경 그림을 다 불러올 때까지 (최대 1.5초) */
  function whenReady(area, slide) {
    const jobs = [...area.querySelectorAll('img')].map(img => (img.decode ? img.decode() : Promise.resolve()).catch(() => {}));
    if (slide && slide.bg && !slide.bg.startsWith('#')) {
      const im = new Image();
      im.src = slide.bg;
      jobs.push((im.decode ? im.decode() : Promise.resolve()).catch(() => {}));
    }
    if (!jobs.length) return Promise.resolve();
    return Promise.race([Promise.all(jobs), new Promise(r => setTimeout(r, 1500))]);
  }

  // 오버레이 층: 슬라이드 전환과 따로, 내용이 바뀔 때만 다시 그린다
  // 오버레이마다 자기 층 (overlay:<id>, 회중 메시지는 message): 바뀐 오버레이만 다시 그린다
  function renderOverlays(p, force) {
    for (const o of p.overlays || []) {
      const el = slot(o.id === 'wpmessage' ? 'message' : o.id === 'wpcaption' ? 'caption' : 'overlay:' + o.id); // 실시간 자막(AI) = caption 칸
      if (!el) continue;
      const key = JSON.stringify([o, tpl.font && tpl.font.family]);
      if (!force && el.dataset.key === key && el.firstChild) continue;
      el.dataset.key = key;
      el.innerHTML = '';
      el.appendChild(R.buildOverlays([o], tpl));
    }
    // 내린 회중 메시지: 층은 구성에 늘 있으므로 비운다
    const msg = slot('message');
    if (msg && !(p.overlays || []).some(o => o.id === 'wpmessage') && msg.firstChild) { msg.innerHTML = ''; msg.dataset.key = ''; }
    // 내린 실시간 자막: 구성에 직접 넣은 caption 칸은 남으므로 비운다
    const cap = slot('caption');
    if (cap && !(p.overlays || []).some(o => o.id === 'wpcaption') && cap.firstChild) { cap.innerHTML = ''; cap.dataset.key = ''; }
  }

  // ---------- 영상 재생 슬라이드 ----------
  // 소리: 소리 내는 출력(코어 payload.audio, 기본 예배당)의 모니터 창(?window=1)에서만 (OBS 브라우저 소스는 소리 없음 → 이중 재생 방지).
  // audio 칸이 없는 옛 코어면 예전처럼 예배당 창에서만
  const soundHere = () => isWindow && (lastPayload && typeof lastPayload.audio === 'boolean' ? lastPayload.audio : output === 'main');
  let endedKey = '';
  // 유튜브 임베드: 코어의 재생 상태(playing · offset · at)에 맞춰 postMessage 로 재생·멈춤·위치·소리를 맞춘다
  function ytSend(f, func, args) { try { f.contentWindow.postMessage(JSON.stringify({ event: 'command', func, args: args || [] }), '*'); } catch (_) {} }
  function syncYouTube(f, p) {
    const st = p.video, s = p.slide, o = { ...(s.video || {}) };
    if (st.volume != null) o.volume = st.volume;
    if (st.loop != null) o.loop = st.loop;
    const y = f._yt || (f._yt = { time: 0, dur: 0, state: -1, ready: false });
    if (!y.ready) return;
    const start = +f.dataset.start || 0;
    const skew = Date.now() - (st.now || Date.now());
    let target = st.offset + (st.playing ? (Date.now() - skew - st.at) / 1000 : 0);
    const len = y.dur > start ? y.dur - start : 0;
    if (len) target = o.loop ? target % len : Math.min(target, Math.max(0, len - 0.3));
    const off = Math.abs(y.time - (start + target));
    if (off > 1.5 || (!st.playing && off > 0.3)) ytSend(f, 'seekTo', [start + target, true]);
    if (soundHere() && o.sound) { ytSend(f, 'unMute'); ytSend(f, 'setVolume', [Math.round(Math.max(0, Math.min(1, o.volume ?? 1)) * 100)]); } else ytSend(f, 'mute');
    if (st.playing && y.state !== 1 && !(len && !o.loop && target >= len - 0.3)) ytSend(f, 'playVideo');
    if (!st.playing && y.state === 1) ytSend(f, 'pauseVideo');
  }
  function ytReport(cmd, key, duration) {
    fetch('/api/command', { method: 'POST', headers: { 'Content-Type': 'application/json', 'X-WorshipCue': '1' }, body: JSON.stringify({ cmd, args: { key, duration } }) }).catch(() => {});
  }
  window.addEventListener('message', (e) => {
    if (!/^https:\/\/www\.youtube(-nocookie)?\.com$/.test(e.origin)) return;
    const f = current && current.querySelector('iframe.bc-yt');
    if (!f || e.source !== f.contentWindow) return;
    let m; try { m = typeof e.data === 'string' ? JSON.parse(e.data) : e.data; } catch (_) { return; }
    if (!m || typeof m !== 'object') return;
    const y = f._yt || (f._yt = { time: 0, dur: 0, state: -1, ready: false });
    if (m.event === 'onReady' || m.event === 'initialDelivery' || m.event === 'infoDelivery') y.ready = true;
    const info = m.info || {};
    if (typeof info.currentTime === 'number') y.time = info.currentTime;
    if (typeof info.playerState === 'number') y.state = info.playerState;
    const lp = lastPayload;
    const key = lp && lp.video && lp.video.key;
    const start = +f.dataset.start || 0;
    if (typeof info.duration === 'number' && info.duration > 0 && info.duration !== y.dur) {
      y.dur = info.duration;
      if (key) ytReport('videoMeta', key, Math.max(0.1, y.dur - start));
    }
    // 끝남(0): 반복이 아니면 코어에 알린다 (끝나면: 그대로 · 검은 화면 · 다음)
    const looped = lp && ((lp.video && lp.video.loop) || (lp.slide && lp.slide.video && lp.slide.video.loop));
    if (info.playerState === 0 && key && key !== endedKey && !looped) {
      endedKey = key;
      ytReport('videoEnded', key, Math.max(0.1, y.dur - start));
    }
    const now = Date.now();
    if (lp && lp.video && lp.slide && lp.slide.kind === 'video' && (m.event === 'onReady' || now - (y.synced || 0) > 1000)) { y.synced = now; syncYouTube(f, lp); }
  });
  function applyVideo(p) {
    const f = current && current.querySelector('iframe.bc-yt');
    if (f && p.video && p.slide && p.slide.kind === 'video') {
      if (!f.dataset.bound) {
        f.dataset.bound = '1';
        // 플레이어가 상태를 알려 주게 한다 (준비될 때까지 몇 번)
        const hello = () => { try { f.contentWindow.postMessage(JSON.stringify({ event: 'listening', id: 'wp', channel: 'widget' }), '*'); } catch (_) {} };
        f.addEventListener('load', () => { hello(); let n = 0; const t = setInterval(() => { if ((f._yt && f._yt.ready) || ++n > 20 || !f.isConnected) clearInterval(t); else hello(); }, 500); });
      }
      syncYouTube(f, p);
      return;
    }
    const v = current && current.querySelector('video.bc-play');
    if (!v || !p.video || !p.slide || p.slide.kind !== 'video') return;
    const o = { ...(p.slide.video || {}) };
    if (p.video.volume != null) o.volume = p.video.volume; // 미디어 패널에서 바꾼 소리 크기
    if (p.video.loop != null) o.loop = p.video.loop;
    R.syncVideo(v, p.video, { sound: soundHere() && o.sound, volume: o.volume ?? 1, loop: !!o.loop });
    // 페이드아웃: 소리와 화면을 서서히
    clearInterval(v._fadeT);
    if (p.video.fade) {
      const f = p.video.fade, skew = Date.now() - (p.video.now || Date.now()), base = v.volume;
      const step = () => { const k = Math.max(0, 1 - (Date.now() - skew - f.at) / f.ms); v.volume = base * k; if (current) current.style.opacity = String(k); if (k <= 0) clearInterval(v._fadeT); };
      step(); v._fadeT = setInterval(step, 50);
    } else if (current) current.style.opacity = '';
    if (!v.dataset.bound) {
      v.dataset.bound = '1';
      const reportMeta = () => {
        if (!current || current.querySelector('video.bc-play') !== v) return;
        const key = lastPayload && lastPayload.video && lastPayload.video.key;
        if (!key || !Number.isFinite(v.duration) || v.duration <= 0) return;
        fetch('/api/command', { method: 'POST', headers: { 'Content-Type': 'application/json', 'X-WorshipCue': '1' }, body: JSON.stringify({ cmd: 'videoMeta', args: { key, duration: v.duration } }) }).catch(() => {});
      };
      v.addEventListener('loadedmetadata', () => {
        if (lastPayload) R.syncVideo(v, lastPayload.video, { sound: soundHere() && o.sound, volume: o.volume ?? 1, loop: !!o.loop });
        reportMeta();
      });
      // 메타데이터가 이벤트 연결 전에 캐시에서 읽힌 경우도 알린다.
      if (v.readyState >= 1) reportMeta();
      v.addEventListener('ended', () => {
        const key = lastPayload && lastPayload.video && lastPayload.video.key;
        if (!key || key === endedKey) return;
        endedKey = key;
        fetch('/api/command', { method: 'POST', headers: { 'Content-Type': 'application/json', 'X-WorshipCue': '1' }, body: JSON.stringify({ cmd: 'videoEnded', args: { key, duration: v.duration } }) }).catch(() => {});
      });
    }
  }

  // ---------- 배경 숨김 · 로고 화면 ----------
  let logoEl = null;
  let mediaEl = null;
  /** 배경 미디어 층 바꾸기 (같은 것이면 그대로 — 카메라·영상이 끊기지 않게) */
  function applyMedia(m) {
    const key = m ? JSON.stringify([m.type, m.deviceId, m.url, m.fit]) : '';
    if (mediaEl && mediaEl.dataset.key === key && mediaEl.isConnected) return;
    const old = mediaEl;
    mediaEl = null;
    if (old) { old.style.opacity = '0'; setTimeout(() => old.remove(), 450); }
    if (!m) return;
    const sl = slot('media'); // 템플릿 배경 바로 위 (구성 차례)
    if (!sl) return;
    const el = R.buildMediaLayer(m);
    el.style.opacity = '0';
    sl.appendChild(el);
    mediaEl = el;
    requestAnimationFrame(() => requestAnimationFrame(() => { el.style.opacity = '1'; }));
  }
  let bgOff = false;
  function applyLayers(p) {
    const L = p.layers || {};
    stage.classList.toggle('hide-bg', !!L.hideBg);
    // 배경 끄기: 가리기만 하지 않고 배경 영상은 멈추고(카메라는 render.js 가 몇 초 뒤 닫음), 다시 켜면 이어서
    const bgVids = () => stage.querySelectorAll('.bc-bg video, video.bc-slidebg, .bc-slidebg video, .bc-medialayer video');
    if (!!L.hideBg !== bgOff) {
      bgOff = !!L.hideBg;
      if (bgOff) bgVids().forEach(v => { if (!v.dataset.cam) v.pause(); });
      else { R.attachCameras(stage); bgVids().forEach(v => { if (!v.dataset.cam) v.play().catch(() => {}); }); }
    }
    applyMedia(L.media || null);
    const want = L.logo ? L.logoImage : '';
    if (logoEl && (!want || logoEl.dataset.img !== want)) { const old = logoEl; logoEl = null; old.style.opacity = '0'; setTimeout(() => old.remove(), 450); }
    const ls = slot('logo-screen'); // 구성에서 본문 · 오버레이 위
    if (want && !logoEl && ls) {
      logoEl = R.buildLogoScreen(want);
      logoEl.dataset.img = want;
      logoEl.style.opacity = '0';
      ls.appendChild(logoEl);
      requestAnimationFrame(() => requestAnimationFrame(() => { if (logoEl) logoEl.style.opacity = '1'; }));
    }
  }

  /** OBS 자막: 글 없는 그림·악보 슬라이드를 우상단에 작게 (예배당 화면 모양 그대로) */
  let pipKey = '';
  function applyPip(p, show) {
    let el = document.getElementById('bcPip');
    const want = p.pip && show && (p.slide.kind === 'image' || p.slide.kind === 'score');
    if (!want) { if (el) el.remove(); pipKey = ''; return; }
    if (!el) { el = document.createElement('div'); el.id = 'bcPip'; el.className = 'bc-pip'; document.body.appendChild(el); }
    const t = p.pip.template;
    el.style.aspectRatio = `${t.width} / ${t.height}`;
    const k = JSON.stringify([p.slide.image, p.slide.bg, p.slide.edit || null, t.id, t.width]);
    if (k === pipKey) return;
    pipKey = k;
    el.innerHTML = '';
    R.renderInto(el, { ...p.slide, caption: '' }, t);
  }
  function paintDrawLayer() {
    const el = stage && tpl && drawState ? slot('draw') : null;
    if (el) R.paintDraw(el, drawState, tpl);
  }
  function render(p) {
    if (p.cameras) R.setCameraPrefs(p.cameras); // 카메라 해상도·프레임·밝기 (조작 화면과 같게)
    queueMicrotask(() => paintDrawLayer()); // 그리기 층 (구성에서 맨 위)
    lastPayload = p;
    document.body.classList.toggle('nocursor', p.cursorHidden !== false);
    document.body.classList.toggle('held', !!p.held); // 보조 화면을 꺼서 OBS 화면도 숨김
    // 그림 우상단을 켰으면, 자막(가사만) 화면에서는 그림을 크게 그리지 않고 우상단에만 (가사가 있으면 가사는 아래에)
    if (p.pip && p.slide && (p.slide.kind === 'image' || p.slide.kind === 'score') && typeof p.slide.caption !== 'string') p = { ...p, slide: { ...p.slide, caption: '' } };
    let stageChanged = setupStage(p.template);
    const fresh = syncLayers(p); // 켜고 끈 오버레이 · 사용자 소스 · 화면 구성에 따라 층을 맞춘다
    // 화면 구성을 고쳐 상자를 새로 만든 칸: 그 소스를 다시 그린다
    if (fresh.has('background')) placeBackground();
    if (fresh.has('live')) lastKey = '';
    if (fresh.has('media')) mediaEl = null;
    if (fresh.has('slide-bg')) { vUrl = ''; vLayer = null; }
    if (fresh.has('logo-screen')) logoEl = null;
    renderUserSources(p);
    renderCams();
    const show = p.hasLive && p.visible && p.slide;
    applyPip(p, show);
    const key = show ? JSON.stringify([p.slide, p.versionName]) : '';
    const bg = stage.querySelector('.bc-bg');
    if (bg) bg.style.opacity = (!show && p.template.clearHidesBackground) ? '0' : '1';

    renderOverlays(p, stageChanged);
    if (stageChanged) { logoEl = null; mediaEl = null; }
    applyLayers(p);
    if (!stageChanged && key === lastKey) { applyVideo(p); ack(p); return; }
    lastKey = key;

    const token = ++renderSeq;
    // 지금 무대에 있는 본문층은 모두 '이전 층' (전환 중에 또 넘겨도 남는 층이 없게)
    const liveSlot = slot('live');
    const olds = liveSlot ? [...liveSlot.querySelectorAll(':scope > .bc-area')] : [];
    current = null;
    if (!show) {
      // 숨김: 디졸브면 서서히 사라짐
      if (p.template.clearHidesBackground || !p.hasLive) setVideoBg('');
      fadeOut(olds, fadeMs(), 0);
      ack(p);
      return;
    }
    // 순서 배경이 영상이면 영상 층에서 재생 (가사 화면 등 글자 슬라이드)
    const lt = slotTpl('live'); // 칸 크기 · 칸 글 모양에 맞춘 템플릿 (기본 구성이면 무대 템플릿 그대로)
    const wantVid = p.slide.bg && R.isVideo(p.slide.bg) && !(R.lyricsOnly(lt) && typeof p.slide.caption === 'string') ? p.slide.bg : '';
    setVideoBg(wantVid);
    const area = R.buildContent(p.slide, lt, p.versionName, { videoBgLayer: true });
    area.style.transition = 'none';
    area.style.opacity = '0';
    if (!liveSlot) { ack(p); return; } // 이 구성에는 순서 슬라이드 칸이 없다
    liveSlot.appendChild(area); // 이전 층 위에 쌓인다
    R.fit(area, lt);
    current = area;
    applyVideo(p);
    // 그림(악보·PPT·배경)을 다 불러온 뒤에 바꾼다 → 빈 화면이 잠깐 보이는 깜빡임 방지
    whenReady(area, p.slide).then(() => {
      if (token !== renderSeq) return; // 그 사이 다음 슬라이드가 왔으면 그쪽이 처리
      const ms = fadeMs();
      if (!ms) { area.style.opacity = '1'; olds.forEach(o => o.remove()); return; }
      if (coversScreen(p.slide, lt)) {
        // 화면을 꽉 채우는 그림: 이전 화면 위로 새 화면이 서서히 덮는다 (진짜 디졸브)
        fadeIn(area, ms, 0);
        olds.forEach(o => { o.style.transition = 'none'; });
        setTimeout(() => olds.forEach(o => o.remove()), ms + 50);
      } else {
        // 글자(배경이 비치는 층): 이전 글이 먼저 사라지고 새 글이 나타난다 → 두 글이 겹쳐 보이지 않음
        const half = Math.round(ms / 2);
        fadeOut(olds, half, 0);
        fadeIn(area, ms - half, olds.length ? half : 0);
      }
    });
    ack(p);
  }

  function ack(p) {
    if (!clientId || !p) return;
    fetch('/api/ack', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json', 'X-WorshipCue': '1' },
      body: JSON.stringify({ id: clientId, version: p.version, width: innerWidth, height: innerHeight, visible: !!(p.visible && p.hasLive) }),
    }).catch(() => {});
  }

  function connect() {
    const es = new EventSource(`/events?client=${output}${isWindow ? '&window=1' : ''}`);
    es.addEventListener('render', e => {
      try { render(JSON.parse(e.data)); } catch (err) { console.error(err); }
    });
    es.addEventListener('draw', e => {
      try { drawState = JSON.parse(e.data); paintDrawLayer(); } catch (err) { console.error(err); }
    });
    es.addEventListener('hello', e => {
      clientId = JSON.parse(e.data).id;
      ack(lastPayload);
    });
    // 오류 시 EventSource 가 자동 재연결한다. 화면은 마지막 상태를 유지한다.
    es.onerror = () => { clientId = null; };
  }

  // 글꼴 로딩 후 크기 맞춤이 정확해지도록 다시 그림
  if (document.fonts && document.fonts.ready) {
    document.fonts.ready.then(() => { if (lastPayload) { lastKey = ''; tplKey = ''; render(lastPayload); } });
    // 나중에 쓰이기 시작한 글꼴(예: PPT 에서 꺼낸 글상자의 구세군체 — local() 별칭)이 다 읽히면 한 번 더
    if (document.fonts.addEventListener) document.fonts.addEventListener('loadingdone', () => { if (lastPayload) { lastKey = ''; tplKey = ''; render(lastPayload); } });
  }
  connect();
})();
