// 로그인 화면. 열쇠는 이 페이지에서 바로 /auth/login 으로만 보내고 어디에도 저장하지 않는다.
(function () {
  'use strict';
  const $ = id => document.getElementById(id);
  const q = new URLSearchParams(location.search);
  // 주소 조각(#church=…&key=…)으로 받은 열쇠: 서버로 가지 않는 자리라 기록에 남지 않는다. 읽고 바로 지운다
  const frag = new URLSearchParams(location.hash.slice(1));
  if (location.hash) history.replaceState(null, '', location.pathname + location.search);
  let church = frag.get('church') || q.get('c') || '';
  try { church = church || decodeURIComponent((document.cookie.match(/(?:^|; )wp_church=([^;]*)/) || [])[1] || ''); } catch (e) { /* 무시 */ }
  $('church').value = church;
  if (church) $('secret').focus(); else $('church').focus();
  /** 돌아갈 곳: 같은 사이트 안의 길만 */
  function next(home) {
    const n = q.get('next') || '';
    return /^\/(?!\/)[^\\]*$/.test(n) ? n : home;
  }
  async function login(c, s) {
    $('go').disabled = true; $('err').textContent = '';
    try {
      // 로그인 이름이 있으면 사람 계정(2단계, /auth/member), 없으면 기기 열쇠 · PIN (1단계)
      const who = ($('login') && $('login').value.trim().toLowerCase()) || '';
      const r = who
        ? await fetch('/auth/member', { method: 'POST', headers: { 'content-type': 'application/json', 'x-worshipcue': '1' }, body: JSON.stringify({ church: c.trim().toLowerCase(), login: who, password: s }) })
        : await fetch('/auth/login', { method: 'POST', headers: { 'content-type': 'application/json', 'x-worshipcue': '1' }, body: JSON.stringify({ church: c.trim().toLowerCase(), secret: s.trim() }) });
      const j = await r.json().catch(() => ({ ok: false, error: '응답을 읽지 못했습니다.' }));
      if (!j.ok) { $('err').textContent = j.error || '들어가지 못했습니다.'; return; }
      location.replace(next(j.home || '/'));
    } catch (e) {
      $('err').textContent = '인터넷 연결을 확인하세요.';
    } finally { $('go').disabled = false; }
  }
  $('f').addEventListener('submit', e => { e.preventDefault(); login($('church').value, $('secret').value); });
  if (frag.get('church') && frag.get('key')) login(frag.get('church'), frag.get('key'));
})();
