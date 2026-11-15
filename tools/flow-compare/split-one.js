// 한 범위의 나눔을 웹·Rust 로 만들어 다른 곳과 그 슬라이드의 높이를 보여 준다
// node tools\flow-compare\split-one.js "롬 8:28-39" count 2 theme-sa-redshield-main,theme-bc-clean-lyrics-overlay niv2011
'use strict';
const fs = require('fs'); const path = require('path'); const { launch } = require('./cdp');
const RSD = 'tmp/rs-data';
const RS = `http://127.0.0.1:${fs.readFileSync(path.join(RSD, 'flow-port.txt'), 'utf8').trim()}`;
const CTL = fs.readFileSync(path.join(RSD, 'flow-ctl.txt'), 'utf8').trim();
const rs = async (cmd, args) => { const r = await (await fetch(CTL, { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ cmd, args }) })).json(); if (!r.ok) throw new Error(r.error); return r.result; };
const [ref, mode, count, tpls, par] = process.argv.slice(2);
const key = s => (s.parts || []).map(v => `${v.chapter}:${v.verse}${v.partCount ? '#' + v.partIndex + '/' + v.partCount : ''}:${(v.text || '').length}`).join(',');
(async () => {
  const br = await launch('tmp/wprofile4');
  try {
    const pg = await br.open(RS + '/outputs/main'); await pg.waitFor('!!window.BCRender');
    await pg.eval(`Promise.all(['/shared/groups.js','/shared/slides.js'].map(u => new Promise((res, rej) => { const s = document.createElement('script'); s.src = u; s.onload = res; s.onerror = rej; document.head.appendChild(s); }))).then(() => document.fonts.ready).then(() => true)`);
    await rs('updateSettings', { patch: { parallelVersionId: par || '' } });
    const r = await rs('bibleBuild', { query: ref, mode, count: Number(count), templates: tpls.split(',') });
    const web = await pg.eval(`BCSlides.build(${JSON.stringify(r.passage)}, ${JSON.stringify({ mode, count: Number(count), templates: r.templates })})`);
    console.log(`웹 ${web.length}장 / Rust ${r.slides.length}장`);
    const wk = web.map(key), rk = r.slides.map(key);
    let d = 0; while (d < wk.length && wk[d] === rk[d]) d++;
    if (d >= Math.max(wk.length, rk.length)) { console.log('같음'); return; }
    console.log('다른 곳', d, '\n 웹 ', wk[d], '\n Rust', rk[d]);
    for (const [who, s] of [['웹 슬라이드', web[d]], ['Rust 슬라이드', r.slides[d]]]) {
      if (!s) continue;
      const sl = { ...s, kind: 'bible', title: r.passage.title };
      for (const t of r.templates) for (const size of [t.font.size, t.font.minSize]) {
        const w = await pg.eval(`(() => { const R = BCRender, t = ${JSON.stringify(t)}; const st = R.buildStage(t); const host = document.createElement('div'); host.className = 'bc-measure'; host.appendChild(st); document.body.appendChild(host); const a = R.buildContent(${JSON.stringify(sl)}, t, ${JSON.stringify(r.passage.versionName)}); st.appendChild(a); const f = R.fit(a, t, ${size}); const b = a.querySelector('.bc-block'); const par = [...a.querySelectorAll('.bc-par')].map(p => p.getBoundingClientRect().height); const out = { h: b.getBoundingClientRect().height, off: b.offsetHeight, area: a.clientHeight, fits: f.fits, text: a.querySelector('.bc-text').getBoundingClientRect().height, par }; host.remove(); return out; })()`);
        const q = await rs('measure', { slide: sl, tpl: t, size, ver: r.passage.versionName });
        console.log(` ${who} · ${t.id} ${size}px: 웹 h=${w.h.toFixed(1)} (본문 ${w.text.toFixed(1)}, 병행 ${w.par.map(x => x.toFixed(1)).join('/')}) 영역 ${w.area} 맞음=${w.fits} | Rust h=${q.h.toFixed(1)} 영역 ${q.area}`);
      }
    }
    console.log(JSON.stringify(web[d] && web[d].parts.map(p => ({ v: p.verse, text: p.text, text2: p.text2 }))).slice(0, 900));
  } finally { br.close(); setTimeout(() => process.exit(0), 200); }
})();
