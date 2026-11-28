// [웹판] 실제 IME 첫 확인: 검색 칸에 '요한복음 3장 16절' 을 두벌식으로
module.exports = async (page, c) => {
  const r = {};
  r.ps1 = c.ps('click:1375,60|wait:300|hangul|keys:dygksqhrdma 3wkd 16wjf|wait:400');
  r.shotComposing = await c.shot('ime-smoke-typed');
  r.dbg1 = await c.dbg();
  r.ps2 = c.ps('key:RETURN|wait:800');
  r.shotEnter = await c.shot('ime-smoke-enter');
  r.dbg2 = await c.dbg();
  delete r.dbg1.cue; delete r.dbg2.cue;
  return r;
};
