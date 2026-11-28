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
  // 7단계: 구글로 시작하기 · 돌아온 까닭 (oauth.mjs back)
  const WHY = {
    name: '교회 이름을 2~60자로 적어 주세요.', 'church-id': '교회 ID 가 올바르지 않거나 쓸 수 없는 이름입니다.', 'church-taken': '이미 있는 교회 ID 입니다.',
    state: '로그인 시간이 지났습니다. 다시 해 주세요.', denied: '구글에서 허락하지 않았습니다.', token: '구글이 로그인을 확인해 주지 않았습니다. 다시 해 주세요.',
    scope: "구글 동의 화면에서 '드라이브 파일' 칸을 켜야 합니다. 다시 해 주세요.", refresh: '구글이 오래 쓸 열쇠를 주지 않았습니다. 다시 해 주세요.',
    'no-account': '이 구글 계정으로 만든 교회가 없습니다. 아래에서 무료로 시작하세요.', 'other-account': '처음 연결한 구글 계정으로 다시 연결해 주세요.',
    'too-many': '이 곳에서 너무 여러 번 가입했습니다. 한 시간 뒤 다시 해 주세요.', full: '오늘 무료 가입이 가득 찼습니다. 내일 다시 해 주세요.',
    owner: '주인으로 로그인한 뒤 다시 연결해 주세요.', google: '구글 연결에 실패했습니다.', exists: '',
  };
  if (q.get('google') && WHY[q.get('google')] !== undefined) $('gerr').textContent = WHY[q.get('google')] || '';
  $('g').addEventListener('submit', ev => {
    ev.preventDefault();
    const n = $('gname').value.trim();
    if (n.length < 2) { $('gerr').textContent = WHY.name; return; }
    location.assign('/auth/google/start?mode=signup&name=' + encodeURIComponent(n));
  });
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
