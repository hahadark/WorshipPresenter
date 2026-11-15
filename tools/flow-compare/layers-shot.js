// 요소 따로 꺼내기 화면 대조: 붙여 넣은 PPT 마다 두 판이 각자 꺼낸 요소(글상자·그림·도형)를 순서의 edits 로 넣어 송출하고 웹 출력(/outputs/main)을 찍어 비교한다.
// 실행: node tools\flow-compare\layers-shot.js --el <포트> --rs-data tmp\rs-data --el-data tmp\el-data --out tmp\layers-out [--max-slides 6]
'use strict';
const fs = require('fs');
const path = require('path');
const { launch, sleep } = require('./cdp');
const png = require('./png');
const A = process.argv.slice(2);
const opt = (k, d) => { const i = A.indexOf('--' + k); return i >= 0 ? A[i + 1] : d; };
const EL = `http://127.0.0.1:${opt('el', '37528')}`;
const RSD = path.resolve(opt('rs-data', 'tmp/rs-data'));
const ELD = path.resolve(opt('el-data', 'tmp/el-data'));
const RS = `http://127.0.0.1:${fs.readFileSync(path.join(RSD, 'flow-port.txt'), 'utf8').trim()}`;
const RSCTL = fs.readFileSync(path.join(RSD, 'flow-ctl.txt'), 'utf8').trim();
const OUT = opt('out', 'tmp/layers-out');
const MAXS = Number(opt('max-slides', '6'));
fs.mkdirSync(OUT, { recursive: true });
const rsRaw = async (cmd, args = {}) => (await fetch(RSCTL, { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ cmd, args }) })).json();
const rs = async (c, a) => { const r = await rsRaw(c, a); if (!r.ok) throw new Error(r.error); return r.result; };
// cue.js pptTextOn 과 같게: 슬라이드마다 { pptBg: true, items }
const editsOf = slides => Object.fromEntries(slides.map((items, k) => [String(k), items.length && items.length <= 60 ? { pptBg: true, items } : null]).filter(x => x[1]));

async function main() {
  const br = await launch(path.join(OUT, 'profile'));
  const report = { started: new Date().toISOString(), items: [] };
  try {
    const ctl = await br.open(`${EL}/control/`, 1600, 950);
    if (!(await ctl.waitFor('!!(window.BCCue && window.BC && window.BC.state)', 30000))) throw new Error('Electron 조작 화면이 안 떴습니다');
    const ecmd = (n, a = {}) => ctl.eval(`BC.cmd(${JSON.stringify(n)}, ${JSON.stringify(a)})`, 120000);
    const [ep, rp] = [await br.open(`${EL}/outputs/main`), await br.open(`${RS}/outputs/main`)];
    let info = await ctl.eval(`BCCue.cueInfo()`);
    await ctl.eval(`BCCue.openCue(${JSON.stringify(info.id)})`);
    await rs('openCue', { id: info.id });
    const screen = { w: 1920, h: 1080 };
    const names = fs.readdirSync(path.join(ELD, 'pasted')).filter(x => /^clip-\d{8}-\d{6}\.pptx$/i.test(x) && fs.existsSync(path.join(RSD, 'pasted', x)));
    for (const [n, name] of names.entries()) {
      const [ef, rf] = [path.join(ELD, 'pasted', name), path.join(RSD, 'pasted', name)];
      const e = (await ecmd('pptTextLayers', { file: ef, screen })).result;
      let r;
      for (let i = 0; i < 240 && !r; i++) { const x = await rsRaw('pptTextLayers', { file: rf, screen }); if (x.ok) r = x.result; else if (!/만드는 중|being/i.test(x.error)) throw new Error(x.error); else await sleep(250); }
      const base = { title: `요소 꺼내기 ${name}`, number: null, background: 'default', obsLyrics: '' };
      await ctl.eval(`BCCue.addItem('hymn', ${JSON.stringify({ ...base, file: ef, edits: editsOf(e.slides), _at: 100000 })})`, 120000);
      const ri = await rs('addItem', { item: { type: 'hymn', ...base, file: rf, edits: editsOf(r.slides) } });
      info = await ctl.eval(`BCCue.cueInfo()`);
      const ei = info.items.length - 1;
      const es = await ctl.eval(`BCCue.slidesFor(${ei})`, 180000);
      const rsl = (await rs('slidesFor', { index: ri })).slides;
      const irep = { name, count: [es.length, rsl.length], shots: [] };
      report.items.push(irep);
      await ctl.eval(`BCCue.sendItem(${ei})`, 120000);
      await rs('sendItem', { index: ri });
      for (let k = 0; k < Math.min(es.length, rsl.length, MAXS); k++) {
        if (k > 0) { await ecmd('nextSlide'); await rs('nextSlide'); }
        await sleep(k === 0 ? 4000 : 2500);
        await ep.shot(); await rp.shot(); await sleep(1200);
        const [a, b] = [png.decode(await ep.shot()), png.decode(await rp.shot())];
        const d = png.diff(a, b);
        const file = path.join(OUT, `${String(n + 1).padStart(2, '0')}-${k + 1}-main.png`);
        fs.writeFileSync(file, png.encode(png.sideBySide(a, b)));
        irep.shots.push({ slide: k + 1, px: Math.round(d.ratio * 10000) / 100, box: d.box, file });
      }
      console.log(`${name}: 장 수 E${es.length}/R${rsl.length} · 그림 차이 ` + irep.shots.map(s => s.px + '%').join(' '));
    }
  } finally {
    fs.writeFileSync(path.join(OUT, 'report.json'), JSON.stringify(report, null, 1));
    br.close();
  }
}
main().then(() => process.exit(0), e => { console.error(e); process.exit(1); });
