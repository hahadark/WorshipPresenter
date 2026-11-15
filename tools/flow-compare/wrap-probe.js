// 줄바꿈 규칙 대조: 같은 글을 폭을 바꿔 가며 놓았을 때 글 덩어리 높이가 웹과 Rust 에서 같은가
// node tools\flow-compare\wrap-probe.js  (launch.js start --no-el 뒤)
'use strict';
const fs = require('fs'); const path = require('path'); const { launch } = require('./cdp');
const RSD = 'tmp/rs-data';
const RS = `http://127.0.0.1:${fs.readFileSync(path.join(RSD, 'flow-port.txt'), 'utf8').trim()}`;
const CTL = fs.readFileSync(path.join(RSD, 'flow-ctl.txt'), 'utf8').trim();
const rs = async (cmd, args) => { const r = await (await fetch(CTL, { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ cmd, args }) })).json(); if (!r.ok) throw new Error(r.error); return r.result; };
const TEXTS = [
  ['text', '가나다,라마바 사아자'], ['text', '가나다(라마바)사아자 차카타'], ['text', '가나다.라마바'], ['text', 'abc-def ghi-jkl mno'], ['text', 'word—word and—more text'],
  ['text', '1,000명이 3:16-21절을 100%의'], ['text', '가나다 “라마바” 사아자'], ['text', 'and/or this/that'], ['text', '가!나?다 라'], ['text', '가나다…라마바 사…아'],
  ['text', '여호와는 나의 목자시니 내게 부족함이 없으리로다'], ['text', 'For God so loved the world that he gave his one and only Son, that whoever believes in him shall not perish but have eternal life.'],
  ['text', '첫 줄\n둘째 줄은 조금 더 깁니다\n\n넷째'], ['text', '**굵은 글** 보통 글 {크기:150}큰 글자{/크기} 다시 보통'], ['text', '주님의 사랑\n> Love of the Lord\n영원하네\n> Forever more'],
  ['text', '[C]주님 [G]사랑 [Am]영원해\n[F]할렐루야'], ['text', 'Twenty-three well-known self-control “quoted text,” he said—then left.'], ['text', '가나다라마바사아자차카타파하가나다라마바사아자차카타파하'],
  ['text', 'Supercalifragilisticexpialidocious and pneumonoultramicroscopicsilicovolcanoconiosis'], ['text', '(셀라) [할렐루야] {아멘} <호산나>'], ['text', '주후 2026년 11월 15일(주일) 오전 11:00'],
  ['bible', [{ verse: 16, text: '하나님이 세상을 이처럼 사랑하사 독생자를 주셨으니' }, { verse: 17, text: '하나님이 그 아들을 세상에 보내신 것은' }]],
  ['bible', [{ verse: 119, text: '가나다 라마바', text2: 'The quick brown fox jumps over the lazy dog' }, { verse: 120, text: '사아자', partIndex: 2, partCount: 3 }]],
  ['title', '주일 낮 예배 순서', '2026년 11월 15일 · 대예배실'],
];
(async () => {
  const br = await launch('tmp/wprofile2');
  try {
    const pg = await br.open(RS + '/outputs/main'); await pg.waitFor('!!window.BCRender');
    await pg.eval(`Promise.all(['/shared/groups.js'].map(u => new Promise((res, rej) => { const s = document.createElement('script'); s.src = u; s.onload = res; s.onerror = rej; document.head.appendChild(s); }))).then(() => document.fonts.ready).then(() => true)`);
    const base = (await rs('templates')).find(t => t.id === 'worship');
    let total = 0, bad = 0;
    for (const variant of [{}, { align: 'hanging' }, { verseBreak: true, font: { ...base.font, letterSpacing: 3, weight: 700 } }, { font: { ...base.font, family: "Georgia, 'Malgun Gothic', serif" } }, { reference: { ...base.reference, show: true, position: 'top' }, versionLabel: { ...base.versionLabel, show: true } }]) {
      for (const [kind, a, b] of TEXTS) {
        const slide = kind === 'text' ? { kind, ref: variant.reference ? '공지 제목이 꽤 길게 들어가는 경우' : '', parts: [{ text: a }] } : kind === 'title' ? { kind, ref: a, subtitle: b, parts: [] } : { kind, ref: '요한복음 3:16-17', title: '요한복음 3:16-21', parts: a.map(p => ({ book: 43, chapter: 3, heading: '', ...p })) };
        const diffs = [];
        const tpls = [];
        for (let W = 120; W <= 1500; W += 23) tpls.push({ ...base, ...variant, width: W, height: 4000, padding: { left: 0, right: 0, top: 0, bottom: 0 }, safeArea: 0, id: 'p' + W });
        const web = await pg.eval(`(() => { const R = BCRender; return ${JSON.stringify(tpls)}.map(t => { const st = R.buildStage(t); const host = document.createElement('div'); host.className = 'bc-measure'; host.appendChild(st); document.body.appendChild(host); const a = R.buildContent(${JSON.stringify(slide)}, t, '개역개정'); st.appendChild(a); R.fit(a, t, 60); const h = a.querySelector('.bc-block').getBoundingClientRect().height; host.remove(); return h; }); })()`, 60000);
        for (let i = 0; i < tpls.length; i++) {
          const q = await rs('measure', { slide, tpl: tpls[i], size: 60, ver: '개역개정' });
          total++;
          if (Math.abs(q.h - web[i]) > 1) { bad++; diffs.push(`W${tpls[i].width}: 웹 ${web[i].toFixed(1)} / Rust ${q.h.toFixed(1)}`); }
        }
        if (diffs.length) console.log(`✗ [${JSON.stringify(variant).slice(0, 40)}] ${kind} ${JSON.stringify(a).slice(0, 50)} — ${diffs.length}곳: ${diffs.slice(0, 4).join(' · ')}`);
      }
    }
    console.log(`줄바꿈 대조: ${bad}/${total} 다름`);
  } finally { br.close(); setTimeout(() => process.exit(0), 200); }
})();
