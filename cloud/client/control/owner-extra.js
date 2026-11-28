// 3단계 (web-egui): 주인 화면에 덧붙이는 두 구획 — 사람(편집자 · 보기만) 넣고 빼기 · 교회 AI(키 · 기능별 동의).
// API 는 2A(identity) · 5(ai) 가 만든 것 그대로: /api/v1/members · /api/v1/ai/settings · /api/v1/ai/consent (docs/web/data-api.md · ai.md)
// 웹 조작 화면의 [교회 관리] 단추가 이 화면(/owner)을 연다.
(function () {
  'use strict';
  const $ = id => document.getElementById(id);
  const esc = s => String(s == null ? '' : s).replace(/[&<>"]/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' }[c]));
  const day = ms => (ms ? new Date(ms).toLocaleString('ko-KR', { dateStyle: 'short', timeStyle: 'short' }) : '—');
  const ROLE = { owner: '주인', editor: '편집자 (고치기 · 리모컨)', viewer: '보기만' };
  async function api(path, opt = {}) {
    const r = await fetch(path, { ...opt, headers: { 'content-type': 'application/json', 'x-worshipcue': '1', ...(opt.headers || {}) } });
    const j = await r.json().catch(() => ({ ok: false, error: '응답을 읽지 못했습니다.' }));
    j._status = r.status;
    return j;
  }

  // ---------- 사람 ----------
  const people = $('wp-people');
  if (people) {
    people.innerHTML = `
      <div class="row">
        <input id="pp-label" placeholder="이름 (예: 찬양팀 김OO)" maxlength="60" style="width:auto;flex:1">
        <input id="pp-login" placeholder="로그인 이름 (영문 · 숫자) 또는 전자우편" maxlength="120" style="width:auto;flex:1">
        <select id="pp-role" style="width:auto"><option value="editor">${ROLE.editor}</option><option value="viewer">${ROLE.viewer}</option><option value="owner">${ROLE.owner}</option></select>
        <button class="primary" type="button" id="pp-add">넣기</button>
      </div>
      <p class="muted">로그인 이름으로 넣으면 처음 비밀번호를 한 번만 보여 줍니다 (그 사람이 바꿀 수 있음). 전자우편으로 넣으면 그 계정으로 로그인합니다.</p>
      <div id="pp-made"></div><p class="err" id="pp-err"></p>
      <table><thead><tr><th>이름</th><th>로그인</th><th>권한</th><th>마지막 로그인</th><th></th></tr></thead><tbody id="pp-list"></tbody></table>`;
    const list = async () => {
      const j = await api('/api/v1/members');
      if (!j.ok) { $('pp-err').textContent = j.error || ''; $('pp-list').innerHTML = ''; return; }
      $('pp-list').innerHTML = j.members.map(m => `<tr><td>${esc(m.label || '(이름 없음)')}</td><td>${esc(m.login || m.email || '')}</td>
        <td><select data-role="${esc(m.id)}">${Object.keys(ROLE).map(r => `<option value="${r}"${r === m.role ? ' selected' : ''}>${esc(ROLE[r])}</option>`).join('')}</select></td>
        <td>${day(m.lastLoginAt)}</td><td>${m.provider === 'password' ? `<button type="button" data-reset="${esc(m.id)}">비밀번호 새로</button> ` : ''}<button type="button" data-del="${esc(m.id)}">빼기</button></td></tr>`).join('');
    };
    const shown = (m, pw) => {
      $('pp-made').innerHTML = pw ? `<div class="secret"><strong>${esc(m.label || m.login)} — 처음 비밀번호</strong> <span class="muted">이 창을 닫으면 다시 볼 수 없습니다.</span>
        <label>교회 ID · 로그인 이름 · 비밀번호</label><div class="row"><code>${esc(m.login)}</code> <code>${esc(pw)}</code> <button type="button" data-copy="${esc(pw)}">복사</button></div></div>` : '';
    };
    $('pp-add').onclick = async () => {
      $('pp-err').textContent = '';
      const v = $('pp-login').value.trim();
      const body = { label: $('pp-label').value.trim(), role: $('pp-role').value, ...(/^[^\s@]+@[^\s@]+\.[^\s@]{2,}$/.test(v) ? { email: v } : { login: v.toLowerCase() }) };
      const j = await api('/api/v1/members', { method: 'POST', body: JSON.stringify(body) });
      if (!j.ok) { $('pp-err').textContent = j.error || ''; return; }
      $('pp-label').value = ''; $('pp-login').value = '';
      shown(j.member, j.password);
      list();
    };
    people.addEventListener('change', async e => {
      const s = e.target.closest('select[data-role]');
      if (!s) return;
      const j = await api('/api/v1/members/' + encodeURIComponent(s.dataset.role), { method: 'PATCH', body: JSON.stringify({ role: s.value }) });
      if (!j.ok) $('pp-err').textContent = j.error || '';
      list();
    });
    people.addEventListener('click', async e => {
      const b = e.target.closest('button');
      if (!b) return;
      if (b.dataset.copy) { try { await navigator.clipboard.writeText(b.dataset.copy); b.textContent = '복사함'; } catch (_) { /* 무시 */ } }
      if (b.dataset.reset) {
        if (!confirm('비밀번호를 새로 만들까요? 그 사람이 로그인해 있던 기기는 끊깁니다.')) return;
        const j = await api('/api/v1/members/' + encodeURIComponent(b.dataset.reset), { method: 'PATCH', body: JSON.stringify({ resetPassword: true }) });
        if (!j.ok) { $('pp-err').textContent = j.error || ''; return; }
        shown(j.member, j.password);
      }
      if (b.dataset.del) {
        if (!confirm('이 사람을 뺄까요? 로그인해 있던 기기는 바로 끊깁니다.')) return;
        const j = await api('/api/v1/members/' + encodeURIComponent(b.dataset.del), { method: 'DELETE' });
        if (!j.ok) $('pp-err').textContent = j.error || '';
        list();
      }
    });
    list();
  }

  // ---------- 교회 AI ----------
  const ai = $('wp-ai');
  if (ai) {
    const draw = s => {
      const prov = (s.providers || []).map(p => `<option value="${esc(p.endpoint)}"${p.endpoint === s.endpointUsed ? ' selected' : ''}>${esc(p.name)}</option>`).join('');
      const custom = (s.providers || []).every(p => p.endpoint !== s.endpointUsed);
      ai.innerHTML = `
        <p class="muted">AI 는 교회 키로 클라우드가 대신 부릅니다. 키는 봉인해 두고 화면 · 기록에 내보내지 않습니다. 사람이 단추를 눌렀을 때만, 동의한 기능만 부릅니다.</p>
        <div class="row"><label style="width:7em">어디로</label><select id="ai-ep" style="width:auto">${prov}<option value=""${custom ? ' selected' : ''}>직접 넣기 (https)</option></select>
          <input id="ai-ep2" placeholder="https://…/chat/completions" value="${custom ? esc(s.endpointUsed) : ''}" style="width:auto;flex:1;${custom ? '' : 'display:none'}"></div>
        <div class="row"><label style="width:7em">모델</label><input id="ai-model" value="${esc(s.model || '')}" placeholder="${esc(s.modelUsed || '')}" style="width:auto;flex:1"></div>
        <div class="row"><label style="width:7em">키</label><input id="ai-key" type="password" autocomplete="off" placeholder="${s.hasKey ? '넣어 둔 키 ' + esc(s.keyHint || '') + ' (바꿀 때만)' : '키를 넣으세요'}" style="width:auto;flex:1">
          ${s.hasKey ? '<button type="button" id="ai-delkey">키 지우기</button>' : ''}</div>
        <div class="row"><label style="width:7em">하루 한도</label><input id="ai-limit" type="number" min="0" max="500" value="${esc(s.dailyLimit)}" style="width:6em"> <span class="muted">번 (오늘 ${esc(s.today?.calls || 0)}번 · 토큰 ${esc(s.today?.tokens || 0)})</span></div>
        <div class="row"><button class="primary" type="button" id="ai-save">저장</button> <span class="muted" id="ai-msg"></span></div>
        <h3>기능별 동의</h3>
        <table><tbody>${(s.features || []).map(f => `<tr><td><label><input type="checkbox" data-feat="${esc(f.id)}"${f.consented ? ' checked' : ''}> ${esc(f.name)}</label></td><td class="muted">${esc(f.what || '')}</td><td class="muted">${f.consented ? esc(day(f.at)) : ''}</td></tr>`).join('')}</tbody></table>
        <p class="err" id="ai-err"></p>`;
      $('ai-ep').onchange = () => { $('ai-ep2').style.display = $('ai-ep').value ? 'none' : ''; };
      $('ai-save').onclick = async () => {
        $('ai-err').textContent = ''; $('ai-msg').textContent = '';
        const body = { endpoint: $('ai-ep').value || $('ai-ep2').value.trim(), model: $('ai-model').value.trim(), dailyLimit: Number($('ai-limit').value) };
        if ($('ai-key').value) body.key = $('ai-key').value;
        const j = await api('/api/v1/ai/settings', { method: 'PUT', body: JSON.stringify(body) });
        if (!j.ok) { $('ai-err').textContent = j.error || ''; return; }
        $('ai-msg').textContent = '저장했습니다.';
        load();
      };
      const del = $('ai-delkey');
      if (del) del.onclick = async () => { if (!confirm('교회 AI 키를 지울까요?')) return; await api('/api/v1/ai/settings', { method: 'PUT', body: JSON.stringify({ key: '' }) }); load(); };
    };
    ai.addEventListener('change', async e => {
      const c = e.target.closest('input[data-feat]');
      if (!c) return;
      const f = c.dataset.feat;
      if (c.checked) {
        const t = await api('/api/v1/ai/consent?feature=' + encodeURIComponent(f));
        if (!confirm((t.text || '이 기능에 동의할까요?') + '\n\n동의할까요?')) { c.checked = false; return; }
      }
      const j = await api('/api/v1/ai/consent', { method: 'POST', body: JSON.stringify({ feature: f, agree: c.checked }) });
      if (!j.ok) { $('ai-err').textContent = j.error || ''; c.checked = !c.checked; }
    });
    const load = async () => {
      const s = await api('/api/v1/ai/settings');
      if (!s.ok) { ai.innerHTML = `<p class="err">${esc(s.error || 'AI 설정을 읽지 못했습니다.')}</p>`; return; }
      draw(s);
    };
    load();
  }
})();
