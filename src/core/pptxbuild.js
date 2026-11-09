'use strict';
// PPT 요소 따로 고치기 (PowerPoint 없이): .pptx/.ppsx → 캐시 폴더에
//   slide-NNN.svg      슬라이드 그림 (full 일 때 — PowerPoint 가 없을 때 '화면 그대로' 슬라이드로 쓴다)
//   slide-NNN-bg.svg   꺼낸 요소를 뺀 배경 그림 (배경 채우기 · 레이아웃·마스터 장식 · 돌린 요소 · 표 · 차트…)
//   slide-NNN-eMM.svg  요소 하나짜리 그림 (선 · 그라데이션 · 미리 정의된 모양 등 글상자·단순 도형으로 못 만드는 것)
//   media-N.ext        PPT 안의 그림 (ppt/media) 그대로
//   layers.json        { v:3, w, h, bgs:[배경 파일…], slides:[[요소…]], hide:{ '1':[도형 id…] }, texts:[슬라이드 글…] }
// 요소(pptlayers.js toItems 가 편집기 글상자·그림·도형으로 바꾼다):
//   글   { kind:'text', l,t,w,h(pt), text, size, bold, italic, color:'#rrggbb', font, fontEa, align 1·2·3, anchor 1·3·4, ml,mr,mt,mb, lh, wrap }
//   그림 { kind:'image', l,t,w,h, file, fit:'cover'|'contain', crop:{l,t,r,b}?, opacity }
//   도형 { kind:'shape', l,t,w,h, fill, ellipse, radius(pt), opacity }
// 무엇을 꺼내고 무엇을 배경에 두는가:
//   꺼냄 — 글만 있는 상자 → 글 · 그림(png·jpg·gif·bmp·webp·svg) → 그림(자르기 포함) · 단색 사각형·둥근 사각형·타원(선 없음) → 도형(+글)
//          그 밖의 도형·선·그라데이션·그림 채우기 → 요소 그림(+글)
//   배경 — 돌리거나 뒤집은 것 · 돌린 그룹 안 · 세로 글 · 표 · 차트 · SmartArt · OLE · 쪽 번호·바닥글·날짜 자리 · emf/wmf 그림 · 레이아웃·마스터 장식

const fs = require('fs');
const path = require('path');
const { openPptx, parseSlide, slideText, textLines } = require('./pptxparse');
const { slideSvg, elementSvg } = require('./pptxsvg');

const MIME = { png: 'image/png', jpg: 'image/jpeg', jpeg: 'image/jpeg', gif: 'image/gif', bmp: 'image/bmp', webp: 'image/webp', svg: 'image/svg+xml' };
const extOf = part => String(part || '').split('.').pop().toLowerCase();
const r2 = n => Math.round(n * 100) / 100;
const HANGUL = /[ㄱ-ㆎ가-힣]/;

/** 그림 부분(part) → 캐시 파일 이름·data: 주소 (같은 그림은 한 번만) */
class Media {
  constructor(pkg, outDir) { this.pkg = pkg; this.outDir = outDir; this.names = new Map(); this.uris = new Map(); this.uriBytes = 0; }
  ok(part) { return !!(part && MIME[extOf(part)] && this.pkg.has(part)); }
  /** 캐시에 복사하고 파일 이름을 돌려준다 (못 그리는 형식이면 null) */
  file(part) {
    if (!this.ok(part)) return null;
    if (this.names.has(part)) return this.names.get(part);
    const name = `media-${this.names.size + 1}.${extOf(part) === 'jpeg' ? 'jpg' : extOf(part)}`;
    fs.writeFileSync(path.join(this.outDir, name), this.pkg.raw(part));
    this.names.set(part, name);
    return name;
  }
  /** SVG 안에 넣을 data: 주소 (작은 그림은 기억해 둔다 — 같은 배경이 수백 장에 쓰인다) */
  uri(part) {
    if (!this.ok(part)) return null;
    if (this.uris.has(part)) return this.uris.get(part);
    const buf = this.pkg.raw(part);
    const uri = `data:${MIME[extOf(part)]};base64,${buf.toString('base64')}`;
    if (this.uriBytes + uri.length < 64 * 1024 * 1024) { this.uris.set(part, uri); this.uriBytes += uri.length; }
    return uri;
  }
}

const ALIGN = { l: 1, ctr: 2, r: 3, just: 1, justLow: 1, dist: 2, thaiDist: 1 };
const ANCHOR = { t: 1, ctr: 3, b: 4, just: 1, dist: 1 };
/** 글 몸통 → 글 요소 (대표 글꼴 = 가장 긴 글 조각) */
function textRecord(sh) {
  const t = sh.text;
  const runs = t.paras.flatMap(p => p.runs).filter(r => !r.br && r.text.trim());
  if (!runs.length) return null;
  const main = runs.reduce((a, b) => (b.text.trim().length > a.text.trim().length ? b : a));
  const firstP = t.paras.find(p => p.runs.some(r => r.text.trim())) || t.paras[0];
  let lh = 0;
  if (firstP.lnSpc) lh = firstP.lnSpc.pct ? firstP.lnSpc.pct : main.sz ? firstP.lnSpc.pts / main.sz / 1.2 : 0;
  const text = textLines(t).join('\n').replace(/\s+$/, '');
  return {
    kind: 'text', id: sh.id, l: r2(sh.x), t: r2(sh.y), w: r2(sh.w), h: r2(sh.h), text,
    size: r2(main.sz || 18), bold: !!main.b, italic: !!main.i, color: main.color || '#000000',
    font: main.latin || '', fontEa: main.ea || '', align: ALIGN[firstP.algn] || 1, anchor: ANCHOR[t.bodyPr.anchor] || 1,
    ml: r2(t.bodyPr.lIns), mr: r2(t.bodyPr.rIns), mt: r2(t.bodyPr.tIns), mb: r2(t.bodyPr.bIns), lh: r2(lh), wrap: t.bodyPr.wrap ? 1 : 0,
  };
}
/** 도형을 어떻게 다룰지: skip(아무것도 안 보임) · bg(배경에 그대로) · text · pic · shape · element */
function classify(sh, media) {
  if (sh.origin !== 'slide') return 'bg';
  if (sh.rot % 360 !== 0 || (sh.gt && sh.gt.length)) return 'bg';
  if (sh.ph && ['sldNum', 'ftr', 'dt'].includes(sh.ph.type)) return 'bg';
  if (['table', 'chart', 'diagram', 'graphic', 'ole'].includes(sh.kind)) return sh.kind === 'chart' || sh.kind === 'graphic' ? (sh.pic ? 'bg' : 'skip') : 'bg';
  const hasText = !!(sh.text && sh.text.hasText);
  if (hasText && sh.text.bodyPr.vert && sh.text.bodyPr.vert !== 'horz') return 'bg';
  if (sh.kind === 'pic') {
    if (!sh.pic || !media.ok(sh.pic.part)) return sh.pic ? 'bg' : 'skip';
    if (sh.flipH || sh.flipV) return 'bg';
    if (sh.geom && sh.geom.prst && sh.geom.prst !== 'rect') return 'element'; // 모양대로 자른 그림
    return 'pic';
  }
  const picFill = sh.fill && sh.fill.type === 'blip';
  const body = !!(sh.fill || sh.line || picFill);
  if (!body) return hasText ? 'text' : 'skip';
  if (sh.flipH || sh.flipV) return 'bg';
  const prst = sh.geom ? sh.geom.prst : 'rect';
  if (['rect', 'roundRect', 'ellipse', 'flowChartProcess', 'flowChartConnector'].includes(prst) && sh.fill && sh.fill.type === 'solid' && !sh.line) return 'shape';
  return 'element';
}

/**
 * 파일 → 캐시 폴더. opts.full = 슬라이드 그림(slide-NNN.svg)도 만든다 (PowerPoint 가 없을 때)
 * → { n, w, h, slides: ['slide-001.svg', …] | null, texts: [글…], raw }
 */
function build(file, outDir, opts = {}) {
  fs.mkdirSync(outDir, { recursive: true });
  const doc = openPptx(file);
  const media = new Media(doc.pkg, outDir);
  const embed = part => media.uri(part);
  const raw = { v: 3, w: r2(doc.w), h: r2(doc.h), bgs: [], slides: [], hide: {}, texts: [] };
  const files = [];
  for (let k = 0; k < doc.slides.length; k++) {
    const n = k + 1;
    const tag = String(n).padStart(3, '0');
    let slide;
    try { slide = parseSlide(doc, k); } catch (e) { slide = { w: doc.w, h: doc.h, bg: { type: 'solid', color: '#ffffff', alpha: 1 }, shapes: [], error: e.message }; }
    const recs = [];
    const skip = new Set();
    const ids = [];
    let m = 0;
    for (const sh of slide.shapes) {
      const how = classify(sh, media);
      if (how === 'bg') continue;
      if (how === 'skip') { skip.add(sh); continue; }
      const box = { l: r2(sh.x), t: r2(sh.y), w: r2(sh.w), h: r2(sh.h) };
      let body = null; // 몸통 요소 (그림·도형·요소 그림) — 글은 그 위에 글 요소로
      if (how === 'pic') {
        body = { kind: 'image', id: sh.id, ...box, file: media.file(sh.pic.part), fit: 'cover', ...(sh.pic.crop ? { crop: sh.pic.crop } : {}), opacity: sh.pic.alpha != null ? sh.pic.alpha : 1 };
      } else if (how === 'shape') {
        const ss = Math.min(sh.w, sh.h);
        const adj = sh.geom && sh.geom.adj && Number.isFinite(sh.geom.adj.adj) ? sh.geom.adj.adj : 16667;
        const prst = sh.geom ? sh.geom.prst : 'rect';
        body = { kind: 'shape', id: sh.id, ...box, fill: sh.fill.color, ellipse: prst === 'ellipse' || prst === 'flowChartConnector', radius: prst === 'roundRect' ? r2(ss * adj / 100000) : 0, opacity: sh.fill.alpha != null ? sh.fill.alpha : 1 };
      } else if (how === 'element') {
        const el = elementSvg(sh, { embed });
        if (el) {
          m++;
          const f = `slide-${tag}-e${String(m).padStart(2, '0')}.svg`;
          fs.writeFileSync(path.join(outDir, f), el.svg);
          body = { kind: 'image', id: sh.id, l: r2(el.l), t: r2(el.t), w: r2(el.w), h: r2(el.h), file: f, fit: 'contain', opacity: 1 };
        }
      }
      const tr = sh.text && sh.text.hasText ? textRecord(sh) : null;
      skip.add(sh); // 꺼냈거나(글·몸통) 보일 것이 없거나 — 배경에서 뺀다
      if (!body && !tr) continue;
      if (body) recs.push(body);
      if (tr) recs.push(tr);
      if (sh.id) ids.push(sh.id);
    }
    // 배경 = 꺼낸 요소를 뺀 슬라이드 · (full) 슬라이드 그림 = 전부
    const bgName = `slide-${tag}-bg.svg`;
    fs.writeFileSync(path.join(outDir, bgName), slideSvg(slide, { skip, embed }));
    if (opts.full) { const name = `slide-${tag}.svg`; fs.writeFileSync(path.join(outDir, name), slideSvg(slide, { embed })); files.push(name); }
    raw.bgs.push(bgName);
    raw.slides.push(recs);
    raw.hide[String(n)] = ids;
    raw.texts.push(slideText(slide));
  }
  fs.writeFileSync(path.join(outDir, 'layers.json'), JSON.stringify(raw));
  return { n: doc.slides.length, w: doc.w, h: doc.h, slides: opts.full ? files : null, texts: raw.texts, raw };
}

module.exports = { build, classify, textRecord, Media };
