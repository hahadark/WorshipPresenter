// [웹판] 실제 Windows 한국어 입력기로 조작 화면의 글 입력을 시험한다 (probe.js --headful --app 와 함께).
// 검색 칸(한 줄) · 큐시트 '교회 소식' 내용 칸(여러 줄). 기대값과 실제값을 남긴다.
// 키는 운영체제 입력(SendInput 가상 키 → 입력기 조합), 마우스 누름은 CDP 로 (WP_OS_MOUSE=1 이면 운영체제 마우스).
//  까닭: 2026-11-28 시험 중 Windows 방화벽 알림 창(PickerHost, 화면 어둡게)이 주 모니터를 덮어 운영체제 마우스가 브라우저에 닿지 않았다.
const SEARCH = [1375, 60], ELSEWHERE = [170, 620], ITEM7 = [150, 383];
module.exports = async (page, c) => {
  const out = [];
  const OS_MOUSE = !!process.env.WP_OS_MOUSE;
  const run = async (actions) => {
    if (OS_MOUSE) return c.ps(actions);
    const logs = []; let buf = [];
    const flush = () => { if (buf.length) { logs.push(c.ps(buf.join('|'))); buf = []; } };
    for (const a of actions.split('|')) {
      const m = /^(dbl)?click:(\d+),(\d+)$/.exec(a);
      if (m) { flush(); await c.click(+m[2], +m[3], m[1] ? 2 : 1); await c.sleep(120); } else buf.push(a);
    }
    flush();
    return logs.join(' ').replace(/\s+/g, ' ');
  };
  const dbg = () => c.dbg();
  const body = async () => { const d = await dbg(); const it = (d.cue?.items || []).find((x) => x.id === 'isample07'); return it ? it.body : null; };
  const search = async () => (await dbg()).search;
  // 내용 칸 누르기: 편집 칸이 스크롤되어 자리가 바뀌므로, 글 칸에 초점이 갈 때까지 몇 자리를 눌러 본다
  const focusBody = async () => {
    for (const y of [657, 640, 620, 605, 670]) {
      await c.click(1000, y); await c.sleep(250);
      const f = (await dbg()).focus;
      if (f && f !== (await searchId())) return f;
    }
    return null;
  };
  let sid = null;
  const searchId = async () => sid;
  const check = async (name, fn, get, want, shot) => {
    let log = '';
    try { log = await fn(); } catch (e) { log = '오류: ' + e.message; }
    await c.sleep(250);
    const got = await get();
    const ok = typeof want === 'function' ? !!want(got) : got === want;
    const f = shot ? await c.shot(shot) : undefined;
    out.push({ name, ok, got, want: typeof want === 'function' ? want.toString() : want, shot: f && f.split(/[\/]/).pop(), log });
    console.error((ok ? 'OK  ' : 'FAIL') + ' ' + name + ' → ' + JSON.stringify(got));
  };
  const clearSearch = `click:${SEARCH}|wait:200|ctrl:A|key:BACK|wait:100`;
  // 입력기 한/영 상태 맞추기: 운영체제가 알려 주는 상태가 브라우저(TSF)와 다를 때가 있어, 'dk' 를 쳐 보고 '아' 가 아니면 한/영 키
  await run(`${clearSearch}|keys:dk|wait:200`);
  sid = (await dbg()).focus;
  const cal = await search();
  if (cal !== '아') await run('key:HANGUL|wait:200');
  out.push({ name: '입력기 맞추기', ok: true, got: cal, log: cal === '아' ? '이미 한글' : '한/영 키 누름' });
  // --- 검색 칸 ---
  await check('검색: 요한복음 3장 16절 + Enter', () => run(`${clearSearch}|keys:dygksqhrdma 3wkd 16wjf|key:RETURN|wait:500`), search, '요한복음 3장 16절', 's1-search-enter');
  await check('검색: 조합 중 Backspace (한그→한ㄱ→한 → 한글, 조합 중 화면)', () => run(`${clearSearch}|keys:gksrm|wait:150|key:BACK|wait:150|key:BACK|wait:150|keys:rmf|wait:150`), search, '한글', 's2-composing');
  await check('검색: 확정 뒤 Backspace (한글 → 한)', () => run(`key:SPACE|key:BACK|key:BACK|wait:150`), search, '한');
  await check('검색: 조합 중 한/영 전환 (요 + abc)', () => run(`${clearSearch}|keys:dy|wait:150|key:HANGUL|wait:150|keys:abc|wait:150|key:HANGUL|wait:150`), search, '요abc', 's3-toggle');
  await check('검색: 조합 중 Enter (요)', () => run(`${clearSearch}|keys:dy|wait:150|key:RETURN|wait:400`), search, '요', 's4-enter-composing');
  await check('검색: 조합 중 마우스로 다른 곳 (한)', () => run(`${clearSearch}|keys:gks|wait:150|click:${ELSEWHERE}|wait:300`), search, '한', 's5-click-away');
  await check('검색: 붙여 넣기 (시편 23편)', () => run(`clip:시편 23편|${clearSearch}|ctrl:V|wait:300`), search, '시편 23편');
  // --- 큐시트 여러 줄 칸 ---
  const base = '다음 주는 추수감사주일입니다.\n---\n예배 후 함께 점심을 나눕니다.';
  await check('편집: 순서 고르기 (처음 내용)', () => run(`click:${ITEM7}|wait:500`), body, base, 'm0-item');
  await check('편집: 줄 끝 Enter · 안녕하세요 · 조합 중 Enter · 감사(조합 중 화면)', async () => { await focusBody(); await run('ctrl:END|key:RETURN|keys:dkssudgktpdy|wait:120|key:RETURN|keys:rkatk|wait:300'); }, async () => (await dbg()).focus ? 'focused' : 'no-focus', 'focused', 'm1-composing');
  await check('편집: 다른 곳 누르면 마지막 음절 한 번만 확정', () => run(`click:${ELSEWHERE}|wait:400`), body, base + '\n안녕하세요\n감사', 'm1-committed');
  await check('편집: 조합 중 Backspace (한글)', async () => { await focusBody(); await run('ctrl:END|key:RETURN|keys:gksrm|wait:120|key:BACK|wait:120|key:BACK|wait:120|keys:rmf|wait:150'); await run(`click:${ELSEWHERE}|wait:300`); }, body, base + '\n안녕하세요\n감사\n한글', 'm2-backspace');
  await check('편집: 여러 줄 붙여 넣기', async () => { await run('clip:첫째 줄\n둘째 줄'); await focusBody(); await run(`ctrl:END|key:RETURN|ctrl:V|wait:300|click:${ELSEWHERE}|wait:300`); }, body, (g) => g === base + '\n안녕하세요\n감사\n한글\n첫째 줄\n둘째 줄', 'm3-paste');
  const before = await body();
  await check('편집: 되돌리기 Ctrl+Z (방금 친 "감사" 가 빠짐)', async () => { await focusBody(); await run('ctrl:END|keys:rkatk|key:SPACE|wait:200|ctrl:Z|wait:300'); await run(`click:${ELSEWHERE}|wait:300`); }, body, (g) => g === before, 'm4-undo');
  await check('편집: 모두 고르고 바꾸기 (새 글)', async () => { await focusBody(); await run(`ctrl:A|keys:to rmf|wait:150|click:${ELSEWHERE}|wait:300`); }, body, '새 글', 'm5-replace');
  await check('편집→검색 오가기: 내용 칸 조합 중 검색 칸 누르고 입력', async () => { await focusBody(); await run(`ctrl:END|keys:gks|wait:150|click:${SEARCH}|wait:250|ctrl:A|key:BACK|keys:dy|wait:200|key:RETURN|wait:300`); }, async () => ({ body: await body(), search: await search() }), (g) => g.body === '새 글한' && g.search === '요', 'm6-cross');
  return out;
};
