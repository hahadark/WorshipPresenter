// 로그인 화면 (쉬운 로그인 — docs/web/login.md). 열쇠 · 비밀번호는 이 페이지에서 바로 보내고 어디에도 저장하지 않는다.
// 1) 구글로 로그인 → /auth/google/start?mode=login (교회 ID 없이, 교회가 여럿이면 고르기)
// 2) 이름 · 비밀번호 → POST /auth/password (교회 ID 없이)
// 3) 고급: 교회 ID + 기기 열쇠 · PIN → POST /auth/login · 사람 계정 + 교회 ID → POST /auth/member (예전 그대로)
(function () {
  'use strict';
  const $ = id => document.getElementById(id);
  const q = new URLSearchParams(location.search);
  const cfg = (() => { try { return JSON.parse($('wp-cfg').textContent || '{}'); } catch (e) { return {}; } })();
  // 주소 조각(#church=…&key=…)으로 받은 열쇠: 서버로 가지 않는 자리라 기록에 남지 않는다. 읽고 바로 지운다
  const frag = new URLSearchParams(location.hash.slice(1));
  if (location.hash) history.replaceState(null, '', location.pathname + location.search);
  let church = frag.get('church') || q.get('c') || '';
  try { church = church || decodeURIComponent((document.cookie.match(/(?:^|; )wp_church=([^;]*)/) || [])[1] || ''); } catch (e) { /* 무시 */ }
  $('church').value = church;
  if (cfg.demo) $('gdemo').hidden = false; // 둘러보기는 DEMO_ENABLED=1 일 때만

  /** 돌아갈 곳: 같은 사이트 안의 길만 */
  const safe = n => (/^\/(?!\/)[^\\]*$/.test(n || '') ? n : '');
  const nextOf = home => safe(q.get('next')) || home || '/';
  const remember = () => $('remember').checked;
  const H = { 'content-type': 'application/json', 'x-worshipcue': '1' };

  // ---------- 구글 ----------
  const invite = /^[A-Za-z0-9]{24,64}$/.test(q.get('invite') || '') ? q.get('invite') : '';
  function googleHref() {
    const p = new URLSearchParams({ mode: 'login' });
    if (!remember()) p.set('remember', '0');
    if (safe(q.get('next'))) p.set('next', q.get('next'));
    if (invite) p.set('invite', invite);
    return '/auth/google/start?' + p;
  }
  $('google').href = googleHref();
  $('remember').addEventListener('change', () => { $('google').href = googleHref(); });
  const WHY = {
    name: '교회 이름을 2~60자로 적어 주세요.', 'church-id': '교회 ID 가 올바르지 않거나 쓸 수 없는 이름입니다.', 'church-taken': '이미 있는 교회 ID 입니다.',
    state: '로그인 시간이 지났습니다. 다시 해 주세요.', denied: '구글에서 허락하지 않았습니다.', token: '구글이 로그인을 확인해 주지 않았습니다. 다시 해 주세요.',
    scope: "구글 동의 화면에서 '드라이브 파일' 칸을 켜야 합니다. 다시 해 주세요.", refresh: '구글이 오래 쓸 열쇠를 주지 않았습니다. 다시 해 주세요.',
    'no-account': '이 구글 계정은 아직 어느 교회에도 없습니다. 교회 주인에게 이 이메일로 초대를 부탁하거나, 아래에서 새 교회를 무료로 시작하세요.',
    'other-account': '처음 연결한 구글 계정으로 다시 연결해 주세요.',
    'too-many': '이 곳에서 너무 여러 번 가입했습니다. 한 시간 뒤 다시 해 주세요.', full: '오늘 무료 가입이 가득 찼습니다. 내일 다시 해 주세요.',
    owner: '주인으로 로그인한 뒤 다시 연결해 주세요.', google: '구글 연결에 실패했습니다.', exists: '',
  };
  const why = q.get('google');
  if (why && WHY[why] !== undefined) { $('gerr2').textContent = WHY[why] || ''; if (['name', 'church-id', 'church-taken', 'too-many', 'full'].includes(why)) { $('new').open = true; $('gerr').textContent = WHY[why]; } }
  $('g').addEventListener('submit', ev => {
    ev.preventDefault();
    const n = $('gname').value.trim();
    if (n.length < 2) { $('gerr').textContent = WHY.name; return; }
    location.assign('/auth/google/start?mode=signup&name=' + encodeURIComponent(n));
  });

  // ---------- 초대 링크 ----------
  const ROLE = { owner: '주인', editor: '편집자', viewer: '보기만' };
  if (invite) {
    fetch('/auth/invite?t=' + invite).then(r => r.json()).then(j => {
      $('invite').hidden = false;
      $('invite-text').textContent = j.ok ? `'${j.church}' 에 ${ROLE[j.role] || j.role}(으)로 초대받았습니다${j.email ? ` (${j.email})` : ''}.` : (j.error || '초대가 없거나 끝났습니다.');
    }).catch(() => {});
  }

  // ---------- 교회 고르기 ----------
  async function showPick() {
    const j = await fetch('/auth/pick').then(r => r.json()).catch(() => ({ ok: false }));
    if (!j.ok) { $('gerr2').textContent = j.error || '고를 시간이 지났습니다. 다시 로그인해 주세요.'; return; }
    $('main').hidden = true;
    $('pick').hidden = false;
    $('pick-list').innerHTML = '';
    for (const c of j.churches) {
      const b = document.createElement('button');
      b.type = 'button'; b.className = 'big';
      b.textContent = `${c.name}  ·  ${ROLE[c.role] || c.role}`;
      b.onclick = async () => {
        b.disabled = true;
        const r = await fetch('/auth/pick', { method: 'POST', headers: H, body: JSON.stringify({ church: c.church }) }).then(x => x.json()).catch(() => ({ ok: false }));
        if (!r.ok) { $('pick-err').textContent = r.error || '들어가지 못했습니다.'; b.disabled = false; return; }
        location.replace(nextOf(safe(j.next) || r.home));
      };
      $('pick-list').appendChild(b);
    }
  }
  if (q.get('pick')) showPick();

  // ---------- 이름 · 비밀번호 ----------
  $('pw').addEventListener('submit', async ev => {
    ev.preventDefault();
    $('pw-go').disabled = true; $('pw-err').textContent = '';
    try {
      const r = await fetch('/auth/password', { method: 'POST', headers: H, body: JSON.stringify({ login: $('pw-login').value.trim().toLowerCase(), password: $('pw-pass').value, remember: remember(), next: safe(q.get('next')) }) });
      const j = await r.json().catch(() => ({ ok: false, error: '응답을 읽지 못했습니다.' }));
      if (!j.ok) { $('pw-err').textContent = j.error || '들어가지 못했습니다.'; return; }
      if (j.pick) return showPick();
      location.replace(nextOf(j.home));
    } catch (e) {
      $('pw-err').textContent = '인터넷 연결을 확인하세요.';
    } finally { $('pw-go').disabled = false; }
  });

  // ---------- 고급: 교회 ID + 열쇠 · PIN (예전 그대로) ----------
  async function login(c, s) {
    $('go').disabled = true; $('err').textContent = '';
    try {
      const who = ($('login') && $('login').value.trim().toLowerCase()) || '';
      const r = who
        ? await fetch('/auth/member', { method: 'POST', headers: H, body: JSON.stringify({ church: c.trim().toLowerCase(), login: who, password: s, remember: remember() }) })
        : await fetch('/auth/login', { method: 'POST', headers: H, body: JSON.stringify({ church: c.trim().toLowerCase(), secret: s.trim() }) });
      const j = await r.json().catch(() => ({ ok: false, error: '응답을 읽지 못했습니다.' }));
      if (!j.ok) { $('err').textContent = j.error || '들어가지 못했습니다.'; return; }
      location.replace(nextOf(j.home));
    } catch (e) {
      $('err').textContent = '인터넷 연결을 확인하세요.';
    } finally { $('go').disabled = false; }
  }
  $('f').addEventListener('submit', e => { e.preventDefault(); login($('church').value, $('secret').value); });
  if (frag.get('church') && frag.get('key')) { $('adv').open = true; login(frag.get('church'), frag.get('key')); }
  if (church && q.get('c')) $('adv').open = true; // 교회가 적힌 주소로 왔으면 (예전 안내) 고급을 열어 둔다
})();
