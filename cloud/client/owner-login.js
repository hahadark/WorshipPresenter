// 주인 화면: 사람 초대(이메일 · 링크) · 로그인한 기기 (쉬운 로그인 — docs/web/login.md)
(function () {
  'use strict';
  const $ = id => document.getElementById(id);
  const esc = s => String(s == null ? '' : s).replace(/[&<>"]/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' }[c]));
  const day = ms => (ms ? new Date(ms).toLocaleString('ko-KR', { dateStyle: 'short', timeStyle: 'short' }) : '—');
  const ROLE = { owner: '주인', editor: '편집자', viewer: '보기만' };
  async function api(path, opt = {}) {
    const r = await fetch(path, { ...opt, headers: { 'content-type': 'application/json', 'x-worshipcue': '1', ...(opt.headers || {}) } });
    if (r.status === 401) { location.replace('/login?next=/owner'); throw new Error('로그인'); }
    return r.json().catch(() => ({ ok: false, error: '응답을 읽지 못했습니다.' }));
  }

  // ---------- 초대 ----------
  async function invites() {
    const j = await api('/api/v1/invites');
    if (!j.ok) { $('iv-err').textContent = j.error || ''; return; }
    $('iv-list').innerHTML = j.invites.length ? j.invites.map(x => `<tr><td>${esc(x.email || '(링크)')}</td><td>${esc(ROLE[x.role] || x.role)}</td>
      <td>${x.usedAt ? '받음 ' + day(x.usedAt) : x.expired ? '끝남' : (x.expiresAt ? day(x.expiresAt) + ' 까지' : '끝 없음')}</td>
      <td>${x.usedAt ? '' : `<button type="button" data-iv="${esc(x.id)}">지우기</button>`}</td></tr>`).join('') : '<tr><td colspan="4" class="muted">받지 않은 초대가 없습니다.</td></tr>';
  }
  async function invite(link) {
    $('iv-err').textContent = ''; $('iv-out').innerHTML = '';
    const email = $('iv-email').value.trim();
    if (!link && !email) { $('iv-err').textContent = '이메일을 적어 주세요 (또는 초대 링크 만들기).'; return; }
    const j = await api('/api/v1/invites', { method: 'POST', body: JSON.stringify({ email: email || undefined, role: $('iv-role').value, link }) });
    if (!j.ok) { $('iv-err').textContent = j.error || ''; return; }
    $('iv-email').value = '';
    $('iv-out').innerHTML = j.url
      ? `<div class="secret"><strong>초대 링크</strong> <span class="muted">(7일 · 한 번만. 이 창을 닫으면 다시 볼 수 없습니다)</span><div class="row"><code>${esc(j.url)}</code> <button type="button" data-copy="${esc(j.url)}">복사</button></div></div>`
      : `<p class="ok">${esc(email)} 님을 ${esc(ROLE[j.invite.role])}(으)로 초대했습니다. 그 구글 계정으로 로그인하면 바로 들어옵니다 (30일).</p>`;
    invites();
  }

  // ---------- 로그인한 기기 ----------
  async function sessions() {
    const j = await api('/api/v1/sessions');
    if (!j.ok) { $('ss-err').textContent = j.error || ''; return; }
    const VIA = { google: '구글', password: '비밀번호', access: 'Access', pick: '' };
    $('ss-list').innerHTML = j.sessions.length ? j.sessions.map(x => `<tr><td>${esc(x.who)} <span class="muted">${esc(ROLE[x.role] || x.role)}</span></td>
      <td>${esc(x.device)}${x.current ? ' <b>(이 기기)</b>' : ''}<div class="muted">${esc(VIA[x.via] ?? x.via)}${x.remember ? ' · 기억함' : ' · 이번만'}</div></td>
      <td>${day(x.lastSeenAt)}</td><td>${x.current ? '' : `<button type="button" data-ss="${esc(x.id)}">로그아웃시키기</button>`}</td></tr>`).join('') : '<tr><td colspan="4" class="muted">사람 계정으로 로그인한 기기가 없습니다.</td></tr>';
  }

  document.addEventListener('click', async e => {
    const b = e.target.closest('button');
    if (!b) return;
    if (b.id === 'iv-send') invite(false);
    if (b.id === 'iv-link') invite(true);
    if (b.dataset.iv) { await api('/api/v1/invites/' + encodeURIComponent(b.dataset.iv), { method: 'DELETE' }); invites(); }
    if (b.dataset.ss) {
      if (!confirm('이 기기를 로그아웃시킬까요? 그 기기에서 열린 화면도 바로 끊깁니다.')) return;
      const j = await api('/api/v1/sessions/' + encodeURIComponent(b.dataset.ss), { method: 'DELETE' });
      if (!j.ok) $('ss-err').textContent = j.error || '';
      sessions();
    }
  });
  invites();
  sessions();
})();
