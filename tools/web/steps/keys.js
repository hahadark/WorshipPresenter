// [웹판] 단축키: 브라우저가 가져가는 키를 실제 키보드(운영체제 입력)로 눌러 본다.
// 페이지가 새로 고쳐졌는지(표식이 사라짐) · 앱 동작이 일어났는지(송출 판 번호 · 검색 칸 초점)를 적는다.
const KEYS = [
  ['F9 송출', 'key:F9'], ['PageDown 다음 장', 'key:PAGEDOWN'], ['Space 다음', 'key:SPACE'], ['PageUp 이전 장', 'key:PAGEUP'],
  ['F6 다음 장', 'key:F6'], ['F5 이전 장', 'key:F5'], ['Ctrl+K 검색', 'ctrl:K'], ['Esc', 'key:ESCAPE'],
  ['F10 가리기', 'key:F10'], ['Alt+Left', 'alt:LEFT'], ['Ctrl+L', 'ctrl:L'], ['Ctrl+R', 'ctrl:R'],
];
module.exports = async (page, c) => {
  const out = [];
  const mark = () => page.eval('window.__mark = (window.__mark||0)+1');
  // 4번 순서(요 3:16-21)를 미리보기
  await c.click(150, 293); await c.sleep(500);
  for (const [name, act] of KEYS) {
    // 매번 큐시트 빈 곳을 눌러 글 칸 초점을 푼다 (글 칸에서는 키가 글로 가므로)
    await c.click(170, 640); await c.sleep(150);
    await mark();
    const before = await c.dbg();
    let log = '';
    try { log = c.ps(act + '|wait:400'); } catch (e) { log = 'ERR ' + e.message.slice(0, 200); }
    await c.sleep(900);
    let alive = false, after = null;
    try { alive = (await page.eval('window.__mark||0')) > 0; after = await c.dbg(); } catch (e) { alive = false; }
    if (!alive) { await c.sleep(4000); try { await page.eval('1'); } catch (e) {} }
    out.push({ name, reloaded: !alive, before: before && before.live, after: after && after.live, focusAfter: after && after.focus });
    console.error(name, alive ? '' : '→ 새로 고침됨!', JSON.stringify(before && before.live), '→', JSON.stringify(after && after.live), after && after.focus);
    if (!alive) { // 새로 고침되었으면 다시 열릴 때까지 기다린 뒤 4번 순서를 다시 고른다
      for (let i = 0; i < 60; i++) { try { if (await page.eval('!!(window.wpTimes&&window.wpTimes.painted)')) break; } catch {} await c.sleep(300); }
      await c.click(150, 293); await c.sleep(500);
    }
  }
  return out;
};
