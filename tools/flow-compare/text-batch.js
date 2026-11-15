// 찬양 가사·공지·교독문 슬라이드 나눔 일괄 대조: 같은 순서·같은 템플릿으로
//   웹(BCSlides.buildText — Electron 판 cue.js buildSlides 와 같은 부름)과 Rust(slides::build_item)의 장 수·나눔 위치를 비교한다.
// 자료: 리허설 큐시트의 찬양·공지 순서 전부 + 성경 본문으로 만든 가사(줄 길이 여러 가지 · 이름표 · 부를 순서 · 번역 줄 · 코드 · 서식)·공지·교독문
// 준비: node tools\flow-compare\launch.js start --no-el
// 실행: node tools\flow-compare\text-batch.js [--rs-data tmp\rs-data] [--out tmp\text-batch] [--label after]
'use strict';
const fs = require('fs');
const path = require('path');
const { launch } = require('./cdp');

const A = process.argv.slice(2);
const opt = (k, d) => { const i = A.indexOf('--' + k); return i >= 0 ? A[i + 1] : d; };
const RSD = opt('rs-data', 'tmp/rs-data');
const RS = `http://127.0.0.1:${fs.readFileSync(path.join(RSD, 'flow-port.txt'), 'utf8').trim()}`;
const RSCTL = fs.readFileSync(path.join(RSD, 'flow-ctl.txt'), 'utf8').trim();
const OUT = opt('out', 'tmp/text-batch');
const LABEL = opt('label', 'run');
fs.mkdirSync(OUT, { recursive: true });

async function rs(cmd, args = {}) {
  const r = await (await fetch(RSCTL, { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ cmd, args }) })).json();
  if (!r.ok) throw new Error(`Rust ${cmd}: ${r.error}`);
  return r.result;
}
const merge = (a, b) => { const o = { ...a }; for (const [k, v] of Object.entries(b)) o[k] = v && typeof v === 'object' && !Array.isArray(v) ? merge(a[k] || {}, v) : v; return o; };

const SETS = [
  { name: '레드 쉴드 + 상자 없는 가사(사용자 현재)', tpl: ['theme-sa-redshield-main', 'theme-bc-clean-lyrics-overlay'] },
  { name: '일반 예배 + 투명 출력', tpl: ['worship', 'overlay'] },
  { name: '성경 봉독(큰 글씨) + 찬송가 두 줄', tpl: ['reading', 'overlay-lyrics'] },
  { name: '설교 하단 자막 + 그라데이션 상자', tpl: ['sermon-lower', 'theme-bc-gradient-box-overlay'] },
  { name: '상자 없는 가사(예배당) + 전체 폭 하단 띠', tpl: ['theme-bc-clean-lyrics-main', 'theme-bc-full-band-overlay'] },
  { name: '전체 폭 하단 띠 + 가운데 작은 상자', tpl: ['theme-bc-full-band-main', 'theme-bc-compact-center-overlay'] },
  { name: '변형: 내어쓰기 · 자간 2 · 줄 간격 1.2 · 굵게', tpl: [{ base: 'worship', patch: { align: 'hanging', font: { letterSpacing: 2, lineHeight: 1.2, weight: 700 } } }, 'overlay-lyrics'] },
  { name: '변형: 큰 글자(96~70) · 상자 · 안전영역 5%', tpl: [{ base: 'worship', patch: { safeArea: 5, box: { enabled: true, padding: 36 }, font: { size: 96, minSize: 70 } } }, 'overlay'] },
  { name: '변형: 나눔고딕 · 자동 맞춤 끔', tpl: [{ base: 'worship', patch: { font: { family: "'NanumGothic', 'Malgun Gothic', 'Apple SD Gothic Neo', sans-serif", autoFit: false, size: 60 } } }, 'overlay'] },
];

// 글을 어절 단위로 n 글자쯤에서 끊어 줄로
const lines = (text, n) => { const out = []; let cur = ''; for (const w of text.split(/\s+/).filter(Boolean)) { if (cur && (cur + ' ' + w).length > n) { out.push(cur); cur = w; } else cur = cur ? cur + ' ' + w : w; } if (cur) out.push(cur); return out; };

async function corpus() {
  const items = [];
  // 1) 리허설 큐시트의 찬양·공지 순서
  for (const id of await rs('listCues')) for (const it of await rs('cueOf', { id })) if ((it.type === 'song' && (it.lyrics || '').trim()) || (it.type === 'text' && (it.body || '').trim())) items.push({ ...it, from: '큐시트' });
  // 2) 성경 본문으로 만든 것 (시편 → 가사, 잠언 → 공지, 시편 → 교독문)
  const verses = async (q, v) => { try { return (await rs('bibleBuild', { query: q, versionId: v || '', mode: 'count', count: 1, templates: ['worship'] })).passage.verses.map(x => x.text); } catch { return []; } };
  for (let k = 1; k <= 60; k++) {
    const ko = await verses(`시 ${k}:1-8`), en = await verses(`시 ${k}:1-8`, 'niv2011');
    if (ko.length < 4) continue;
    const n = [8, 12, 16, 22, 30][k % 5];
    const stanza = (vs, withTr) => vs.flatMap((t, i) => { const l = lines(t, n); return withTr && en[i] ? l.flatMap((x, j) => (j === 0 ? [x, '> ' + lines(en[i], 40)[0]] : [x])) : l; });
    let lyrics = `[1절]\n${stanza(ko.slice(0, 2), k % 7 === 0).join('\n')}\n\n[후렴]\n${stanza(ko.slice(2, 4), false).join('\n')}\n\n[2절]\n${stanza(ko.slice(4, 6), k % 7 === 0).join('\n')}\n\n${stanza(ko.slice(6, 8), false).join('\n')}`;
    if (k % 6 === 0) lyrics = lyrics.replace(/^([^\[>\n])/gm, '[C]$1').replace(/ ([^ \n]+)$/gm, ' [G/B]$1');
    if (k % 9 === 0) lyrics = lyrics.replace(/후렴\]\n([^\n]+)/, '후렴]\n**$1**').replace(/2절\]\n([^\n]+)/, '2절]\n{크기:140}$1{/크기}');
    items.push({ type: 'song', title: `시편 ${k}편 찬양`, lyrics, order: k % 4 === 0 ? '1 C 2 C C' : '', showTitle: k % 3 === 0, from: '만든 가사' });
    // 영어 가사
    if (k % 5 === 0 && en.length >= 4) items.push({ type: 'song', title: `Psalm ${k}`, lyrics: en.slice(0, 6).map((t, i) => (i % 2 ? '' : `[${i / 2 + 1}]\n`) + lines(t, 34).join('\n')).join('\n\n'), order: '', showTitle: false, from: '만든 영어 가사' });
  }
  for (let k = 1; k <= 31; k++) {
    const ko = await verses(`잠 ${k}:1-12`);
    if (ko.length < 6) continue;
    const body = k % 3 === 0 ? ko.slice(0, 4).join('\n') + '\n---\n' + ko.slice(4, 8).join('\n') + '\n---\n' + ko.slice(8).join('\n')
      : k % 3 === 1 ? ko.map((t, i) => `${i + 1}. ${t}`).join('\n')
        : `교회 소식 ${k}\n\n` + ko.slice(0, 6).map(t => '· ' + lines(t, 26).join('\n  ')).join('\n') + `\n\n문의: 사무실 (053) 000-${1000 + k}`;
    items.push({ type: 'text', title: `광고 ${k}`, body, from: '만든 공지' });
  }
  for (let k = 100; k <= 130; k++) {
    const ko = await verses(`시 ${k}:1-10`);
    if (ko.length < 4) continue;
    const body = k % 2 ? ko.map((t, i) => `${i % 2 ? '회중' : '인도자'}: ${t}`).join('\n') + '\n다같이: 아멘'
      : ko.slice(0, 8).map((t, i) => (i % 2 ? `회중:\n${lines(t, 20).join('\n')}\n` : `인도자: ${t}`)).join('\n') + (k % 4 === 0 ? '\n---\n다 같이: ' + ko[0] : '');
    items.push({ type: 'text', title: `교독문 ${k}`, body, roleLabels: k % 3 === 0, from: '만든 교독문' });
  }
  return items;
}

const key = s => `${s.kind}|${s.ref || ''}|${s.label || ''}|${(s.parts || []).map(p => p.text).join('¶')}`;

async function main() {
  const br = await launch(path.join(OUT, 'profile'));
  const rep = { label: LABEL, at: new Date().toISOString(), sets: [], total: 0, mismatch: 0 };
  try {
    const pg = await br.open(`${RS}/outputs/main`);
    await pg.waitFor('!!window.BCRender', 15000);
    await pg.eval(`Promise.all(['/shared/groups.js','/shared/slides.js'].map(u => new Promise((res, rej) => { const s = document.createElement('script'); s.src = u; s.onload = res; s.onerror = rej; document.head.appendChild(s); }))).then(() => document.fonts.ready).then(() => !!window.BCSlides)`);
    const items = await corpus();
    const kinds = {};
    for (const it of items) kinds[it.from] = (kinds[it.from] || 0) + 1;
    console.log('자료:', JSON.stringify(kinds), '합', items.length);
    const allTpl = await rs('templates');
    const only = opt('sets', '') ? opt('sets', '').split(',').map(Number) : SETS.map((_, i) => i);
    for (const si of only) {
      const set = SETS[si];
      const tplArg = set.tpl.map(x => (typeof x === 'string' ? x : { ...merge(allTpl.find(t => t.id === x.base), x.patch), id: x.base + '~' + si }));
      const srep = { name: set.name, total: 0, mismatch: 0, slides: 0, by: {}, examples: [] };
      rep.sets.push(srep);
      for (const it of items) {
        const r = await rs('itemBuild', { item: it, templates: tplArg });
        const web = await pg.eval(`(() => { const SL = BCSlides, it = ${JSON.stringify(it)}, opts = { mode: 'auto', count: 2, templates: ${JSON.stringify(r.templates)} };
          let s = it.type === 'song' ? SL.buildText({ title: it.title, showTitle: it.showTitle, chunks: SL.arrange(SL.lyricsChunks(it.lyrics), it.order || '').chunks }, opts)
            : SL.buildText({ title: it.title, showTitle: true, chunks: SL.textChunks(it.body, { roles: it.roleLabels ? 'labels' : true }) }, opts);
          if (!s.length) s = [{ kind: 'title', ref: it.title, subtitle: it.subtitle || '', label: '제목', parts: [] }];
          return s; })()`, 120000);
        const wk = web.map(key), rk = r.slides.map(key);
        srep.total++; srep.slides += web.length;
        const b = srep.by[it.from] || (srep.by[it.from] = { total: 0, mismatch: 0 });
        b.total++;
        if (JSON.stringify(wk) !== JSON.stringify(rk)) {
          srep.mismatch++; b.mismatch++;
          let d = 0; while (wk[d] === rk[d]) d++;
          if (srep.examples.length < 10) srep.examples.push({ title: it.title, from: it.from, web: web.length, rust: r.slides.length, at: d, webKey: (wk[d] || '').slice(0, 160), rustKey: (rk[d] || '').slice(0, 160) });
        }
      }
      rep.total += srep.total; rep.mismatch += srep.mismatch;
      console.log(`[${set.name}] ${srep.mismatch}/${srep.total} 순서 불일치 (슬라이드 ${srep.slides}장) ${JSON.stringify(Object.fromEntries(Object.entries(srep.by).filter(([, v]) => v.mismatch).map(([k, v]) => [k, v.mismatch + '/' + v.total])))}`);
    }
    rep.rate = rep.total ? Math.round(rep.mismatch / rep.total * 10000) / 100 : 0;
    console.log(`== ${LABEL}: 전체 ${rep.mismatch}/${rep.total} 불일치 (${rep.rate}%)`);
  } finally {
    fs.writeFileSync(path.join(OUT, `report-${LABEL}.json`), JSON.stringify(rep, null, 1));
    br.close();
    setTimeout(() => process.exit(0), 300);
  }
}
main().catch(e => { console.error(e); process.exit(1); });
