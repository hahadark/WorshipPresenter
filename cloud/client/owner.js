// 주인 화면: 설치판 연결 상태 · 기기 열쇠 만들기 · 지우기. 만든 열쇠 원문은 이 화면에 한 번만 보이고 서버에는 해시만 남는다.
(function () {
  'use strict';
  const $ = id => document.getElementById(id);
  const cfg = JSON.parse($('wp-cfg').textContent || '{}');
  const ROLE = { owner: '주인', bridge: '설치판 연결', output: '출력 화면 · OBS', remote: '리모컨', editor: '웹 편집기' };
  const esc = s => String(s == null ? '' : s).replace(/[&<>"]/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' }[c]));
  const day = ms => (ms ? new Date(ms).toLocaleString('ko-KR', { dateStyle: 'short', timeStyle: 'short' }) : '—');
  $('church').textContent = '교회 ID: ' + (cfg.church || '');
  async function api(path, opt = {}) {
    const r = await fetch(path, { ...opt, headers: { 'content-type': 'application/json', 'x-worshipcue': '1', ...(opt.headers || {}) } });
    if (r.status === 401) { location.replace('/login?next=/owner'); throw new Error('로그인'); }
    return r.json().catch(() => ({ ok: false, error: '응답을 읽지 못했습니다.' }));
  }
  async function room() {
    const j = await api('/api/v1/room').catch(() => null);
    if (!j || !j.ok) { $('room').innerHTML = '<span class="err">상태를 읽지 못했습니다.</span>'; return; }
    const v = j.viewers || [];
    const by = {};
    for (const x of v) by[x.client] = (by[x.client] || 0) + 1;
    $('room').innerHTML = (j.online
      ? `<div><span class="dot on"></span>설치판 연결됨 <span class="muted">(${esc(j.app)} · ${day(j.since)} 부터)</span></div>`
      : '<div><span class="dot"></span>설치판이 연결되어 있지 않습니다. 설치판 → 연결 → 클라우드 연결 에서 켜세요.</div>')
      + `<div class="muted" style="margin-top:6px">붙어 있는 화면 ${v.length}개${v.length ? ' — ' + Object.entries(by).map(([k, n]) => `${esc(k)} ${n}`).join(' · ') : ''}</div>`;
  }
  async function creds() {
    const j = await api('/api/v1/credentials');
    if (!j.ok) { $('err').textContent = j.error || ''; return; }
    $('creds').innerHTML = j.credentials.map(c => `<tr><td>${esc(c.label || '(이름 없음)')}</td><td>${esc(ROLE[c.role] || c.role)}</td><td>${day(c.createdAt)}</td><td>${day(c.lastUsedAt)}</td><td><button type="button" data-del="${esc(c.id)}">지우기</button></td></tr>`).join('');
  }
  function shown(c) {
    const o = location.origin, ch = cfg.church;
    const rows = [];
    if (c.role === 'output') {
      for (const [n, p] of [['예배당 화면', 'main'], ['OBS 투명 자막', 'overlay'], ['무대 화면', 'stage']]) rows.push([n, `${o}/outputs/${p}?key=${c.key}`]);
    } else if (c.role === 'bridge') {
      rows.push(['클라우드 주소', o], ['연결 열쇠', c.key]);
    } else if (c.role === 'remote') {
      rows.push(['리모컨 주소', `${o}/remote?c=${ch}`], ['PIN', c.secret]);
    } else if (c.role === 'editor') {
      rows.push(['편집기 주소', `${o}/editor?c=${ch}`], ['PIN', c.secret]);
    } else rows.push(['교회 ID', ch], ['주인 열쇠', c.secret]);
    $('made').innerHTML = `<div class="secret"><strong>${esc(ROLE[c.role])} 열쇠를 만들었습니다.</strong> <span class="muted">이 창을 닫으면 다시 볼 수 없습니다. 필요한 곳에 지금 넣으세요.</span>`
      + rows.map(([n, v]) => `<label>${esc(n)}</label><div class="row"><code>${esc(v)}</code> <button type="button" data-copy="${esc(v)}">복사</button></div>`).join('') + '</div>';
  }
  $('make').onclick = async () => {
    $('err').textContent = '';
    const j = await api('/api/v1/credentials', { method: 'POST', body: JSON.stringify({ role: $('role').value, label: $('label').value }) });
    if (!j.ok) { $('err').textContent = j.error || ''; return; }
    $('label').value = '';
    shown(j.credential);
    creds();
  };
  document.addEventListener('click', async e => {
    const b = e.target.closest('button');
    if (!b) return;
    if (b.dataset.copy) { try { await navigator.clipboard.writeText(b.dataset.copy); b.textContent = '복사함'; } catch (_) { /* 무시 */ } }
    if (b.dataset.del) {
      if (!confirm('이 열쇠를 지울까요? 그 열쇠로 열린 화면은 바로 끊깁니다.')) return;
      const j = await api('/api/v1/credentials/' + encodeURIComponent(b.dataset.del), { method: 'DELETE' });
      if (!j.ok) $('err').textContent = j.error || '';
      creds();
    }
  });
  $('logout').onclick = async () => { await api('/auth/logout', { method: 'POST' }).catch(() => {}); location.replace('/login'); };
  room(); creds();
  setInterval(() => { if (!document.hidden) room(); }, 15000); // 숨긴 탭은 묻지 않는다 (요청 수)
})();
