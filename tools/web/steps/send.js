// [웹판] 송출 흐름: 출력 탭 열기(BroadcastChannel) → 큐시트 순서 고르기(미리보기) → 송출 → 출력 화면 그림 · 연결 수
module.exports = async (page, c) => {
  const r = {};
  const out = await c.tab2('http://127.0.0.1:37610/outputs/main');
  await c.sleep(1500);
  r.outputBefore = await out.shot('out-before');
  await c.click(150, 293); await c.sleep(600);          // 4. 본문 요 3:16-21 → 미리보기
  r.preview = await c.shot('ctl-preview');
  await c.click(310, 60); await c.sleep(1200);          // 송출 단추
  r.sent = await c.shot('ctl-sent');
  r.output = await out.shot('out-live');
  r.outText = await out.eval('document.getElementById("screen").innerText.slice(0,200)');
  await c.key('PageDown', 'PageDown', 34); await c.sleep(1000);    // 다음 장
  r.output2 = await out.shot('out-next');
  r.outText2 = await out.eval('document.getElementById("screen").innerText.slice(0,200)');
  // 새로 연 출력 탭은 sync 로 지금 화면을 받는다
  const late = await c.tab2('http://127.0.0.1:37610/outputs/main');
  await c.sleep(1500);
  r.lateText = await late.eval('document.getElementById("screen").innerText.slice(0,200)');
  r.stage = await c.tab2('http://127.0.0.1:37610/outputs/stage', 1280, 720).then(async (s) => { await c.sleep(1500); const f = await s.shot('out-stage'); return f; });
  await c.sleep(2500);
  r.ctlAfter = await c.shot('ctl-outputs');
  r.outLogs = out.logs.slice(-5);
  return r;
};
