// 성경 슬라이드 나눔 일괄 대조: 같은 본문·같은 템플릿으로
//   웹(BCSlides.build — Electron 판이 쓰는 DOM 측정 그대로, web/shared 는 Electron 판과 같은 파일)과
//   Rust(slides::build_bible — measure.rs 로 잰다)의 장 수·나눔 위치를 비교해 불일치율을 낸다. Electron 을 띄울 필요가 없다.
// 준비: node tools\flow-compare\launch.js start --no-el
// 실행: node tools\flow-compare\split-batch.js [--out tmp\split-batch] [--heights] [--limit 50] [--sets 0,1] [--label before]
'use strict';
const fs = require('fs');
const path = require('path');
const { launch } = require('./cdp');

const A = process.argv.slice(2);
const opt = (k, d) => { const i = A.indexOf('--' + k); return i >= 0 ? A[i + 1] : d; };
const RSD = opt('rs-data', 'tmp/rs-data');
const RS = `http://127.0.0.1:${fs.readFileSync(path.join(RSD, 'flow-port.txt'), 'utf8').trim()}`;
const RSCTL = fs.readFileSync(path.join(RSD, 'flow-ctl.txt'), 'utf8').trim();
const OUT = opt('out', 'tmp/split-batch');
const LABEL = opt('label', 'run');
fs.mkdirSync(OUT, { recursive: true });

async function rs(cmd, args = {}) {
  const r = await (await fetch(RSCTL, { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ cmd, args }) })).json();
  if (!r.ok) throw new Error(`Rust ${cmd}: ${r.error}`);
  return r.result;
}

// 66권 × 범위 3가지 + 긴 절 (없는 범위는 건너뜀)
const BOOKS = '창 출 레 민 신 수 삿 룻 삼상 삼하 왕상 왕하 대상 대하 스 느 에 욥 시 잠 전 아 사 렘 애 겔 단 호 욜 암 옵 욘 미 나 합 습 학 슥 말 마 막 눅 요 행 롬 고전 고후 갈 엡 빌 골 살전 살후 딤전 딤후 딛 몬 히 약 벧전 벧후 요일 요이 요삼 유 계'.split(' ');
const REFS = [];
for (const b of BOOKS) REFS.push(`${b} 1:1-10`, `${b} 1:11-18`, `${b} 2:1-12`, `${b} 3:5-9`);
REFS.push('에 8:9', '단 5:23', '렘 44:17-19', '수 8:33', '왕하 16:15', '계 20:4', '롬 8:28-39', '창 1:26-2:3', '시 119:1-16', '시 23', '요 3:16', '마 5:3-12', '고전 13', '엡 1:3-14', '빌 2:5-11', '사 53', '시 1', '시 150', '눅 2:1-20', '요 1:1-18');

// 템플릿 묶음(예배당 + OBS 자막) · 병행 번역
const SETS = [
  { name: '레드 쉴드 + 상자 없는 가사(사용자 현재)', tpl: ['theme-sa-redshield-main', 'theme-bc-clean-lyrics-overlay'], par: '' },
  { name: '일반 예배 + 투명 출력', tpl: ['worship', 'overlay'], par: '' },
  { name: '성경 봉독(큰 글씨) + 찬송가 두 줄', tpl: ['reading', 'overlay-lyrics'], par: '' },
  { name: '설교 하단 자막 + 그라데이션 상자', tpl: ['sermon-lower', 'theme-bc-gradient-box-overlay'], par: '' },
  { name: '전체 폭 하단 띠 + 가운데 작은 상자', tpl: ['theme-bc-full-band-main', 'theme-bc-compact-center-overlay'], par: '' },
  { name: '사용자 설정 + 레드 쉴드 OBS', tpl: ['custom', 'theme-sa-redshield-overlay'], par: '' },
  { name: '레드 쉴드 + 상자 없는 가사 · 병행 NIV', tpl: ['theme-sa-redshield-main', 'theme-bc-clean-lyrics-overlay'], par: 'niv2011' },
  { name: '성경 봉독 + 찬송가 두 줄 · 병행 NIV', tpl: ['reading', 'overlay-lyrics'], par: 'niv2011' },
  { name: '일반 예배 + 투명 출력 · 병행 쉬운성경', tpl: ['worship', 'overlay'], par: 'easy' },
  // ---- 템플릿 변형 (기본 템플릿에 없는 꾸밈: { base, patch } 는 base 템플릿에 patch 를 덮어쓴 것) ----
  { name: '변형: 내어쓰기(hanging)', tpl: [{ base: 'worship', patch: { align: 'hanging' } }, { base: 'overlay', patch: { align: 'hanging' } }], par: '' },
  { name: '변형: 절마다 줄바꿈 · 자간 2 · 줄 간격 1.2', tpl: [{ base: 'worship', patch: { verseBreak: true, font: { letterSpacing: 2, lineHeight: 1.2 } } }, 'overlay'], par: '' },
  { name: '변형: 번역본 표시 · 장절 아래(전체 범위) · 안전영역 5% · 소제목', tpl: [{ base: 'worship', patch: { safeArea: 5, versionLabel: { show: true }, reference: { position: 'bottom', mode: 'passage' }, headings: { show: true } } }, 'overlay'], par: '' },
  { name: '변형: 절 번호 숨김 · 상자 · 큰 글자(80~50)', tpl: [{ base: 'worship', patch: { verseNumbers: { show: false }, box: { enabled: true, padding: 40 }, font: { size: 80, minSize: 50 } } }, 'overlay'], par: '' },
  { name: '변형: 바탕체', tpl: [{ base: 'worship', patch: { font: { family: "'Batang', '바탕', serif", weight: 400 } } }, 'overlay'], par: '' },
  { name: '변형: 굴림 굵게 · 병행 NIV', tpl: [{ base: 'reading', patch: { font: { family: "'Gulim', '굴림', sans-serif", weight: 700 } } }, 'overlay'], par: 'niv2011' },
  { name: '변형: 맑은 고딕 가늘게(300) + 굵게(800) · 내어쓰기 · 병행 NIV', tpl: [{ base: 'worship', patch: { align: 'hanging', font: { weight: 300 } } }, { base: 'overlay', patch: { font: { weight: 800 } } }], par: 'niv2011' },
  { name: '변형: 영어 본문(NIV) · Georgia', tpl: [{ base: 'worship', patch: { font: { family: "Georgia, 'Malgun Gothic', serif", weight: 400 } } }, 'overlay'], par: '', version: 'niv2011' },
  { name: '영어 본문(NIV) · 일반 예배 + 투명 출력', tpl: ['worship', 'overlay'], par: '', version: 'niv2011' },
];
const MODES = [{ mode: 'count', count: 1 }, { mode: 'count', count: 2 }, { mode: 'count', count: 3 }, { mode: 'auto', count: 2 }];

const merge = (a, b) => { const o = { ...a }; for (const [k, v] of Object.entries(b)) o[k] = v && typeof v === 'object' && !Array.isArray(v) ? merge(a[k] || {}, v) : v; return o; };
const key = s => (s.parts || []).map(v => `${v.chapter}:${v.verse}${v.partCount ? '#' + v.partIndex + '/' + v.partCount : ''}:${(v.text || '').length}`).join(',');

async function main() {
  const br = await launch(path.join(OUT, 'profile'));
  const rep = { label: LABEL, at: new Date().toISOString(), sets: [], total: 0, mismatch: 0 };
  const hstat = []; // 높이 차 (웹 − Rust)
  try {
    const pg = await br.open(`${RS}/outputs/main`);
    await pg.waitFor('!!window.BCRender', 15000);
    await pg.eval(`Promise.all(['/shared/groups.js','/shared/slides.js'].map(u => new Promise((res, rej) => { const s = document.createElement('script'); s.src = u; s.onload = res; s.onerror = rej; document.head.appendChild(s); }))).then(() => document.fonts.ready).then(() => !!window.BCSlides)`);
    const limit = Number(opt('limit', '0')) || REFS.length;
    const only = opt('sets', '') ? opt('sets', '').split(',').map(Number) : SETS.map((_, i) => i);
    const allTpl = await rs('templates');
    const every = Number(opt('heights-every', '1'));
    for (const si of only) {
      const set = SETS[si];
      const tplArg = set.tpl.map(x => (typeof x === 'string' ? x : { ...merge(allTpl.find(t => t.id === x.base), x.patch), id: x.base + '~' + si }));
      await rs('updateSettings', { patch: { parallelVersionId: set.par } });
      const srep = { name: set.name, modes: {}, total: 0, mismatch: 0, examples: [] };
      rep.sets.push(srep);
      for (const m of MODES) {
        let total = 0, bad = 0;
        for (const ref of REFS.slice(0, limit)) {
          let r;
          try { r = await rs('bibleBuild', { query: ref, versionId: set.version || '', mode: m.mode, count: m.count, templates: tplArg }); } catch { continue; }
          if (!r.passage.verses || !r.passage.verses.length) continue;
          const web = await pg.eval(`BCSlides.build(${JSON.stringify(r.passage)}, ${JSON.stringify({ mode: m.mode, count: m.count, templates: r.templates })})`, 120000);
          const wk = web.map(key), rk = r.slides.map(key);
          total++;
          if (JSON.stringify(wk) !== JSON.stringify(rk)) {
            bad++;
            if (srep.examples.length < 12) { let d = 0; while (wk[d] === rk[d]) d++; srep.examples.push({ ref, mode: m.mode + m.count, web: web.length, rust: r.slides.length, at: d, webKey: wk[d], rustKey: rk[d] }); }
          }
          if (A.includes('--heights') && m.mode === 'auto' && total % every === 0) {
            // 웹이 만든 슬라이드마다, 템플릿마다: 설정 크기·최소 크기에서 글 덩어리 높이
            const hs = await pg.eval(`(() => { const R = BCRender, out = []; const slides = ${JSON.stringify(web.map(s => ({ ...s, title: r.passage.title })))}; const tpls = ${JSON.stringify(r.templates)}; const ver = ${JSON.stringify(r.passage.versionName)};
              for (const t of tpls) { const st = R.buildStage(t); const host = document.createElement('div'); host.className = 'bc-measure'; host.appendChild(st); document.body.appendChild(host);
                for (const s of slides) for (const size of [t.font.size, t.font.minSize]) { const a = R.buildContent(s, t, ver); st.appendChild(a); R.fit(a, t, size); out.push({ t: t.id, size, h: a.querySelector('.bc-block').getBoundingClientRect().height, area: a.clientHeight }); a.remove(); }
                host.remove(); }
              return out; })()`, 120000);
            let k = 0;
            for (const t of r.templates) for (const s of web) for (const size of [t.font.size, t.font.minSize]) {
              const q = await rs('measure', { slide: { ...s, kind: 'bible', title: r.passage.title }, tpl: t, size, ver: r.passage.versionName });
              const w = hs[k++];
              hstat.push({ set: si, t: t.id, ref, size, web: w.h, rust: q.h, area: w.area, rarea: q.area, lineH: size * t.font.lineHeight });
            }
          }
        }
        srep.modes[m.mode + m.count] = { total, mismatch: bad };
        srep.total += total; srep.mismatch += bad;
        console.log(`[${set.name}] ${m.mode}${m.count}: ${bad}/${total} 불일치`);
      }
      rep.total += srep.total; rep.mismatch += srep.mismatch;
    }
    rep.rate = rep.total ? Math.round(rep.mismatch / rep.total * 10000) / 100 : 0;
    console.log(`== ${LABEL}: 전체 ${rep.mismatch}/${rep.total} 불일치 (${rep.rate}%)`);
    if (hstat.length) {
      const by = {};
      for (const h of hstat) { const d = h.rust - h.web; const b = by[h.t] || (by[h.t] = { n: 0, exact: 0, lineOff: 0, sum: 0, max: 0, areaDiff: 0 }); b.n++; if (Math.abs(d) < 1.5) b.exact++; if (Math.abs(d) >= h.lineH * 0.9) b.lineOff++; b.sum += d; b.max = Math.max(b.max, Math.abs(d)); b.areaDiff = Math.max(b.areaDiff, Math.abs(h.area - h.rarea)); }
      for (const [t, b] of Object.entries(by)) console.log(`높이 ${t}: ${b.n}개 · 1.5px 안 ${Math.round(b.exact / b.n * 100)}% · 한 줄 이상 차 ${Math.round(b.lineOff / b.n * 100)}% · 평균 차 ${Math.round(b.sum / b.n * 10) / 10}px · 최대 ${Math.round(b.max)}px · 영역 차 ${b.areaDiff}`);
      rep.heights = by;
      fs.writeFileSync(path.join(OUT, `heights-${LABEL}.json`), JSON.stringify(hstat));
    }
  } finally {
    fs.writeFileSync(path.join(OUT, `report-${LABEL}.json`), JSON.stringify(rep, null, 1));
    br.close();
    setTimeout(() => process.exit(0), 300);
  }
}
main().catch(e => { console.error(e); process.exit(1); });
