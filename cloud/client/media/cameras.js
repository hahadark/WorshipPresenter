// /cameras — 교회 카메라 이름 ↔ 이 PC 장치 짝 정하기 (media.js 가 같은 저장소 'wp-cloud-camera-map' 을 읽는다). 계약: docs/web/media.md
(function () {
  'use strict';
  const $ = id => document.getElementById(id);
  const STORE = 'wp-cloud-camera-map';
  const esc = s => String(s == null ? '' : s).replace(/[&<>"]/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' }[c]));
  const normLabel = s => String(s || '').toLowerCase().replace(/\s*\([0-9a-f]{4}:[0-9a-f]{4}\)\s*$/i, '').replace(/\s+/g, ' ').trim();
  const readMap = () => { try { return JSON.parse(localStorage.getItem(STORE) || '{}') || {}; } catch { return {}; } };
  const writeMap = m => { try { localStorage.setItem(STORE, JSON.stringify(m)); } catch { $('err').textContent = '이 브라우저에 저장할 수 없습니다.'; } };
  let church = {}; // 출력 자료의 cameras
  let local = [];
  const streams = new Map();

  // 교회 카메라 이름: 예배당 화면 자료(cameras)를 실시간 연결로 한 번 받는다
  function loadChurch() {
    const u = new URL('/events?client=main', location.href);
    u.protocol = location.protocol === 'https:' ? 'wss:' : 'ws:';
    const ws = new WebSocket(u.href);
    ws.onmessage = e => {
      let m; try { m = JSON.parse(e.data); } catch { return; }
      if (m.t === 'render' && m.data) { church = m.data.cameras || {}; draw(); ws.close(); }
      if (m.t === 'hello' && !m.online) $('state').textContent = '설치판(또는 웹 조작 화면)이 연결되어 있지 않아 교회 카메라 이름을 아직 모릅니다.';
    };
  }
  async function listLocal() {
    try {
      let all = await navigator.mediaDevices.enumerateDevices();
      if (!all.some(d => d.kind === 'videoinput' && d.label)) {
        const s = await navigator.mediaDevices.getUserMedia({ video: true, audio: false });
        s.getTracks().forEach(t => t.stop());
        all = await navigator.mediaDevices.enumerateDevices();
      }
      local = all.filter(d => d.kind === 'videoinput');
      $('state').textContent = `이 PC 카메라 ${local.length}대`;
    } catch (e) { $('err').textContent = '카메라를 볼 수 없습니다: ' + (e.name || e); }
    draw();
  }
  function draw() {
    const map = readMap();
    const ids = Object.keys(church).filter(id => !id.startsWith('phone:') && church[id] && church[id].label);
    if (!ids.length) { $('rows').innerHTML = '<tr><td colspan="3" class="muted">교회 카메라가 없습니다 (설치판 → 연결 → 카메라 설정에서 이름을 붙이면 여기 보입니다).</td></tr>'; return; }
    $('rows').innerHTML = ids.map(id => {
      const name = church[id].label, key = normLabel(name), cur = map[key] || '';
      const opts = ['<option value="">(같은 이름 자동)</option>'].concat(local.map(d => `<option value="${esc(d.label)}"${d.label === cur ? ' selected' : ''}>${esc(d.label)}</option>`)).join('');
      return `<tr><td>${esc(name)}</td><td><select data-key="${esc(key)}" data-name="${esc(name)}" style="width:auto">${opts}</select></td><td><video data-prev="${esc(key)}" muted playsinline autoplay style="width:160px;height:90px;background:#000"></video></td></tr>`;
    }).join('');
    for (const sel of document.querySelectorAll('select[data-key]')) preview(sel);
  }
  async function preview(sel) {
    const key = sel.dataset.key;
    const label = sel.value || sel.dataset.name;
    const want = local.find(d => d.label === label) || local.find(d => normLabel(d.label) === normLabel(label));
    const v = document.querySelector(`video[data-prev="${CSS.escape(key)}"]`);
    if (streams.has(key)) { streams.get(key).getTracks().forEach(t => t.stop()); streams.delete(key); }
    if (!v || !want) return;
    try {
      const s = await navigator.mediaDevices.getUserMedia({ video: { deviceId: { exact: want.deviceId }, width: { ideal: 640 } }, audio: false });
      streams.set(key, s);
      v.srcObject = s;
    } catch (e) { v.title = '열지 못함: ' + (e.name || e); }
  }
  document.addEventListener('change', e => {
    const sel = e.target.closest('select[data-key]');
    if (!sel) return;
    const m = readMap();
    if (sel.value) m[sel.dataset.key] = sel.value; else delete m[sel.dataset.key];
    writeMap(m);
    preview(sel);
  });
  $('allow').onclick = listLocal;

  // 휴대폰 링크 (쓰기 권한이 있을 때만 됨 — 출력 열쇠로 들어온 PC 는 403)
  $('plink').onclick = async () => {
    const r = await fetch('/api/v1/media/phone-link', { method: 'POST', headers: { 'content-type': 'application/json', 'x-worshipcue': '1' }, body: JSON.stringify({ name: $('pname').value }) });
    const j = await r.json().catch(() => ({}));
    if (!j.ok) { $('err').textContent = j.error || '링크를 만들지 못했습니다.'; return; }
    $('pout').innerHTML = `휴대폰에서 이 주소를 여세요 (${new Date(j.expiresAt).toLocaleString('ko-KR')} 까지): <br><code>${esc(j.url)}</code>`;
  };
  async function phones() {
    const j = await fetch('/rtc/phones').then(r => r.json()).catch(() => null);
    if (j && j.ok) $('phones').textContent = j.phones.length ? '붙어 있는 휴대폰: ' + j.phones.map(p => `${p.name || p.id}${p.size ? ' ' + p.size : ''}`).join(' · ') : '붙어 있는 휴대폰 없음';
  }
  loadChurch();
  phones();
  setInterval(() => { if (!document.hidden) phones(); }, 15000);
})();
