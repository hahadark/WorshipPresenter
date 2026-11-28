// [웹판] CDP 합성 입력으로 성경 검색 칸에 한글 조합(IME) 흉내 → 결과 화면. 실제 Windows IME 시험을 대신하지 않는다.
module.exports = async (page, c) => {
  const r = {};
  await c.click(1375, 60); await c.sleep(300);
  // '요3:16' — 조합: ㅇ → 요 → 확정, 이어서 3:16 은 그냥 글자
  await c.ime('ㅇ'); await c.sleep(120); await c.ime('요'); await c.sleep(120);
  r.composing = await c.shot('search-composing');
  await c.type('요'); await c.sleep(100);
  await c.type('3:16'); await c.sleep(600);
  r.typed = await c.shot('search-typed');
  await c.key('Enter', 'Enter', 13, 0, '\r'); await c.sleep(800);
  r.entered = await c.shot('search-enter');
  return r;
};
