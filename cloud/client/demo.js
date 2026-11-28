// 둘러보기 화면: 견본 교회의 문서 목록 · 성경 찾기. 자료는 /api/v1/* 그대로 (등급과 상관없이 같은 길).
(function () {
  'use strict';
  const $ = id => document.getElementById(id);
  const cfg = JSON.parse($('wp-cfg').textContent || '{}');
  const esc = s => String(s == null ? '' : s).replace(/[&<>"]/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' }[c]));
  const KIND = { cue: '큐시트', song: '찬양', settings: '설정', template: '템플릿', theme: '테마' };
  if (cfg.reset) $('reset').textContent = '방금 처음 모습으로 되돌렸습니다.';
  async function api(path, opt = {}) {
    const r = await fetch(path, { ...opt, headers: { 'content-type': 'application/json', 'x-worshipcue': '1', ...(opt.headers || {}) } });
    if (r.status === 401) { location.replace('/demo'); throw new Error('로그인'); }
    return r.json().catch(() => ({ ok: false, error: '응답을 읽지 못했습니다.' }));
  }
  async function docs() {
    const j = await api('/api/v1/documents');
    if (!j.ok) { $('err').textContent = j.error || ''; return; }
    $('docs').innerHTML = j.documents.map(d => `<tr><td>${esc(KIND[d.kind] || d.kind)}</td><td>${esc(d.title || d.id)}</td><td>${d.revision}</td></tr>`).join('');
  }
  $('bf').addEventListener('submit', async ev => {
    ev.preventDefault();
    const j = await api('/api/v1/bible/passage?version=DEMO&ref=' + encodeURIComponent($('ref').value));
    $('verses').innerHTML = j.ok
      ? `<h3>${esc(j.passage.title)}</h3>` + j.passage.verses.map(v => `<p><sup>${v.verse}</sup> ${esc(v.text)}</p>`).join('')
      : `<p class="err">${esc(j.error)}</p>`;
  });
  $('logout').onclick = async () => { await fetch('/auth/logout', { method: 'POST', headers: { 'x-worshipcue': '1' } }); location.replace('/login'); };
  docs();
})();
