// [웹판] 움직임 재기: 마우스를 큐시트·검색 결과 위로 3초 움직이며 화면 그리기 간격(requestAnimationFrame)을 잰다
module.exports = async (page, c) => {
  await page.eval(`window.__f=[];(function L(t){__f.push(t);if(__f.length<100000)requestAnimationFrame(L)})(performance.now());1`);
  // 검색해서 결과 목록을 띄운 뒤 그 위로
  await c.click(1375, 60); await c.type('사랑'); await c.key('Enter', 'Enter', 13, 0, '\r'); await c.sleep(600);
  await page.eval('__f.length=0;1');
  const t0 = Date.now();
  for (let i = 0; Date.now() - t0 < 3000; i++) {
    const x = 380 + (i * 13) % 800, y = 200 + (i * 7) % 560;
    await page.send('Input.dispatchMouseEvent', { type: 'mouseMoved', x, y });
    await c.sleep(16);
  }
  const f = await page.eval(`(()=>{const d=[];for(let i=1;i<__f.length;i++)d.push(__f[i]-__f[i-1]);d.sort((a,b)=>a-b);const q=p=>d[Math.floor(d.length*p)]||0;return {frames:d.length,avg:d.reduce((a,b)=>a+b,0)/d.length,p50:q(.5),p95:q(.95),max:d[d.length-1]}})()`);
  await c.shot('frames');
  return f;
};
