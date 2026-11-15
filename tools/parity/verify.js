// 조작 화면 그림과 실제 웹 렌더러를 같은 입력으로 대조한다.
// 준비: --parity <cases.json> --parity-out <폴더> --parity-hold 로 점검 앱 실행.
// 사용: node tools/parity/verify.js --out tmp/audit-integrated-parity
// 보고서의 픽셀 차이는 48/255 초과인 화소 비율이다. 0은 모든 픽셀이 정확히 같다는 뜻이 아니다.
'use strict';
const fs = require('fs');
const path = require('path');
const { launch, sleep } = require('../flow-compare/cdp');
const png = require('../flow-compare/png');
const args = process.argv.slice(2);
const i = args.indexOf('--out');
if (i < 0 || !args[i + 1]) throw new Error('--out <점검 폴더>가 필요합니다');
const out = path.resolve(args[i + 1]);

async function main() {
  const deadline = Date.now() + 180000;
  while (!fs.existsSync(path.join(out, 'done.txt'))) {
    if (Date.now() > deadline) throw new Error('조작 화면 촬영이 3분 안에 끝나지 않았습니다');
    await sleep(500);
  }
  const cases = JSON.parse(fs.readFileSync(path.join(out, 'resolved.json'), 'utf8'));
  const sizes = new Map(fs.readFileSync(path.join(out, 'fit.txt'), 'utf8').trim().split(/\r?\n/).map(line => {
    const j = line.lastIndexOf(' ');
    return [line.slice(0, j), Number(line.slice(j + 1))];
  }));
  const port = fs.readFileSync(path.join(out, 'port.txt'), 'utf8').trim();
  const script = fs.readFileSync(path.join(__dirname, 'web-render.js'), 'utf8');
  const browser = await launch(path.join(out, 'verify-browser'));
  const results = [];
  try {
    const page = await browser.open(`http://127.0.0.1:${port}/outputs/main`, 1920, 1080);
    if (!await page.waitFor('!!window.BCRender')) throw new Error('실제 웹 출력 렌더러가 준비되지 않았습니다');
    await page.eval(script);
    for (const c of cases) {
      const rust = png.decode(fs.readFileSync(path.join(out, `rust-${c.name}.png`)));
      const webSize = await page.eval(`window.parityRender(${JSON.stringify(c)})`);
      await sleep(200);
      const web = png.decode(await page.shot(path.join(out, `web-${c.name}.png`)));
      const pixels = png.diff(web, rust, 48);
      const row = { name: c.name, rustSize: sizes.get(c.name), webSize, sizeMatches: sizes.get(c.name) === webSize,
        webDimensions: [web.w, web.h], rustDimensions: [rust.w, rust.h], changedPixelFraction: pixels.ratio, box: pixels.box,
        dynamicPreviewOnly: ['video', 'camera'].includes(c.slide?.kind) };
      results.push(row);
      if (web.w === rust.w && web.h === rust.h) fs.writeFileSync(path.join(out, `compare-${c.name}.png`), png.encode(png.sideBySide(web, rust)));
      console.log(`${c.name}: 크기 ${row.rustSize}/${webSize}, 화소 ${(pixels.ratio * 100).toFixed(3)}%${row.dynamicPreviewOnly ? ' (동영상/카메라 제외 대상)' : ''}`);
    }
    await page.close();
    const staticCases = results.filter(r => !r.dynamicPreviewOnly);
    const summary = { cases: results.length, staticCases: staticCases.length,
      matchingSizes: staticCases.filter(r => r.sizeMatches).length,
      maxChangedPixelFraction: Math.max(...staticCases.map(r => r.changedPixelFraction)),
      dimensionFailures: results.filter(r => r.webDimensions.join() !== r.rustDimensions.join()).map(r => r.name) };
    fs.writeFileSync(path.join(out, 'verify-report.json'), JSON.stringify({ timestamp: new Date().toISOString(), summary, results }, null, 2));
    console.log(JSON.stringify(summary));
    if (summary.dimensionFailures.length || summary.matchingSizes !== summary.staticCases) process.exitCode = 1;
  } finally {
    browser.close();
    fs.writeFileSync(path.join(out, 'close.txt'), '웹 출력 비교 완료');
  }
}
main().catch(error => { console.error(error.stack); process.exitCode = 1; });
