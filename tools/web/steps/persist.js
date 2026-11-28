// [웹판] 저장 층: 큐시트 글을 고치고 새로 고침 → 고친 글이 남는가 (localStorage wpdocs:v1)
module.exports = async (page, c) => {
  const body = async () => { const d = await c.dbg(); const it = (d.cue?.items || []).find((x) => x.id === 'isample07'); return it ? it.body : null; };
  await c.click(150, 383); await c.sleep(500);
  const before = await body();
  for (const y of [657, 640, 620]) { await c.click(1000, y); await c.sleep(250); if ((await c.dbg()).focus) break; }
  await c.key('End', 'End', 35, 2); await c.sleep(100);    // Ctrl+End
  await c.type('\n웹판 저장 시험'); await c.sleep(300);
  await c.click(170, 620); await c.sleep(1800);              // 1초 모아 저장
  const edited = await body();
  const stored = await page.eval('(localStorage.getItem("wpdocs:v1")||"").length');
  await page.send('Page.reload', { ignoreCache: false });
  for (let i = 0; i < 100; i++) { await c.sleep(200); try { if (await page.eval('!!(window.wpTimes&&window.wpTimes.painted)')) break; } catch {} }
  await c.sleep(800);
  const after = await body();
  await c.shot('persist-after-reload');
  return { before, edited, stored, after, ok: after === edited && edited !== before };
};
