// [웹판] 둘째 모니터: '창 배치' 권한을 준 뒤 예배당 화면 켜기 → 출력 창이 다른 화면으로 옮겨지는가 (CDP 로 창 자리를 읽음)
module.exports = async (page, c) => {
  const r = {};
  const origin = await page.eval('location.origin');
  try { await page.send('Browser.grantPermissions', { origin, permissions: ['windowManagement'] }); r.granted = true; } catch (e) { r.granted = String(e.message); }
  r.screens = await page.eval('(async () => { try { const d = await getScreenDetails(); return d.screens.map(s => [s.availLeft, s.availTop, s.availWidth, s.availHeight, s.isPrimary]); } catch (e) { return String(e); } })()');
  r.extended = await page.eval('screen.isExtended');
  // 송출(F9 자리 단추) → '송출하면 예배당 화면 켜기' 가 창을 연다
  await c.click(150, 293); await c.sleep(400);
  await c.click(310, 60); await c.sleep(2500);
  const list = await (await fetch(c.base + '/json/list')).json();
  const out = list.find((t) => t.type === 'page' && /outputs\/main/.test(t.url));
  r.outputTarget = out && out.url;
  if (out) {
    try { const w = await page.send('Browser.getWindowForTarget', { targetId: out.id }); r.bounds = w.bounds; } catch (e) { r.bounds = String(e.message); }
  }
  r.shot = await c.shot('monitor-control');
  return r;
};
