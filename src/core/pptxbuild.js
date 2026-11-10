'use strict';
// PPT 요소 따로 고치기 (PowerPoint 없이): .pptx/.ppsx → 캐시 폴더에
//   slide-NNN.svg      슬라이드 그림 (full 일 때 — PowerPoint 가 없을 때 '화면 그대로' 슬라이드로 쓴다)
//   slide-NNN-bg.svg   꺼낸 요소를 뺀 배경 그림 (배경 채우기 · 레이아웃·마스터 장식 · 돌린 요소 · 표 · 차트 · SmartArt…)
//   slide-NNN-eMM.svg  요소 하나짜리 그림 (선 · 그라데이션 · 미리 정의된 모양 등 글상자·단순 도형으로 못 만드는 것)
//   media-N.ext        PPT 안의 그림 (ppt/media) 그대로 — SVG 는 이 파일 이름을 href 로 가리킨다 (서버가 보낼 때 data: 로 바꿔 넣음, hymns.inlineSvg)
//   layers.json        { v:4, w, h, engine, bgs:[배경 파일…], slides:[[요소…]], hide:{ '1':[도형 id…] }, texts:[슬라이드 글…] }
// 요소(pptlayers.js toItems 가 편집기 글상자·그림·도형으로 바꾼다):
//   글   { kind:'text', l,t,w,h(pt), text, size, bold, italic, color:'#rrggbb', font, fontEa, align 1·2·3, anchor 1·3·4, ml,mr,mt,mb, lh, wrap }
//   그림 { kind:'image', l,t,w,h, file, fit:'cover'|'contain', crop:{l,t,r,b}?, opacity }
//   도형 { kind:'shape', l,t,w,h, fill, ellipse, radius(pt), opacity }
// 무엇을 꺼내고 무엇을 배경에 두는가:
//   꺼냄 — 글만 있는 상자 → 글 · 그림(png·jpg·gif·bmp·webp·svg) → 그림(자르기 포함) · 단색 사각형·둥근 사각형·타원(선 없음) → 도형(+글)
//          그 밖의 도형·선·그라데이션·그림 채우기 → 요소 그림(+글)
//   배경 — 돌리거나 뒤집은 것 · 돌린 그룹 안 · 세로 글 · WordArt · 표 · 차트 · SmartArt · OLE · 쪽 번호·바닥글·날짜 자리 · emf/wmf 그림 · 레이아웃·마스터 장식
// 읽는 엔진: 이음오피스 PPTX 엔진(pptxengine.js — 글꼴 메트릭으로 PowerPoint 처럼 줄바꿈 · 차트 · SmartArt · 표 스타일 · 자동 번호)이 먼저,
//   파일을 못 열거나 슬라이드 하나에서 실패하면 직접 쓴 파서(pptxparse.js · pptxsvg.js)로 그 부분만 돌아간다 (engine: 'ieum' | 'own' | 'mixed').

const fs = require('fs');
const path = require('path');
const { openPptx, parseSlide, slideText, textLines } = require('./pptxparse');
const { slideSvg, elementSvg } = require('./pptxsvg');

const MIME = { png: 'image/png', jpg: 'image/jpeg', jpeg: 'image/jpeg', gif: 'image/gif', bmp: 'image/bmp', webp: 'image/webp', svg: 'image/svg+xml' };
const extOf = part => String(part || '').split('.').pop().toLowerCase();
const r2 = n => Math.round(n * 100) / 100;
const HANGUL = /[ㄱ-ㆎ가-힣]/;

/** 그림 부분(part) → 캐시 파일 이름·data: 주소 (같은 그림은 한 번만). pkg = { has(part), raw(part) → Buffer } */
class Media {
  constructor(pkg, outDir) { this.pkg = pkg; this.outDir = outDir; this.names = new Map(); this.uris = new Map(); this.uriBytes = 0; this.sizes = new Map(); }
  ok(part) { return !!(part && MIME[extOf(part)] && this.pkg.has(part)); }
  /** 캐시에 복사하고 파일 이름을 돌려준다 (못 그리는 형식이면 null) */
  file(part) {
    if (!this.ok(part)) return null;
    if (this.names.has(part)) return this.names.get(part);
    const name = `media-${this.names.size + 1}.${extOf(part) === 'jpeg' ? 'jpg' : extOf(part)}`;
    const buf = this.pkg.raw(part);
    fs.writeFileSync(path.join(this.outDir, name), buf);
    this.names.set(part, name);
    try { const { imageSizeFromBytes } = require('./vendor/ieum-pptx'); const s = imageSizeFromBytes(buf); if (s) this.sizes.set(name, s); } catch { /* 크기 모름 */ }
    return name;
  }
  /** 파일 이름 → 픽셀 크기 { w, h } (png · jpg 만) */
  size(name) { return this.sizes.get(name); }
  /** SVG 안에 넣을 data: 주소 (작은 그림은 기억해 둔다 — 같은 배경이 수백 장에 쓰인다) — 직접 쓴 파서 경로에서만 */
  uri(part) {
    if (!this.ok(part)) return null;
    if (this.uris.has(part)) return this.uris.get(part);
    const buf = this.pkg.raw(part);
    const uri = `data:${MIME[extOf(part)]};base64,${buf.toString('base64')}`;
    if (this.uriBytes + uri.length < 64 * 1024 * 1024) { this.uris.set(part, uri); this.uriBytes += uri.length; }
    return uri;
  }
}

// ---------------- 직접 쓴 파서(pptxparse.js) 경로 ----------------
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
  // 문단 사이 간격(spcBef · spcAft): 편집기 글상자는 줄 간격 하나뿐이라, 짧은 문단(가사·목록 한 줄)이 대부분일 때만 평균 간격을 줄 간격에 더한다
  //   (더하지 않으면 PowerPoint 기본 본문 — 줄 90% · 문단 앞 10pt — 이 뽑아 낸 뒤 빽빽하게 보인다)
  const textParas = t.paras.filter(p => p.runs.some(r => !r.br && r.text.trim()));
  if (textParas.length > 1 && main.sz) {
    const gapPts = v => (!v ? 0 : v.pts != null ? Number(v.pts) || 0 : v.pct != null ? (Number(v.pct) || 0) * main.sz * 1.2 : 0);
    const gaps = textParas.slice(1).map((p, i) => gapPts(p.spcBef) + gapPts(textParas[i].spcAft));
    const avg = gaps.reduce((a, b) => a + b, 0) / gaps.length;
    const shortParas = textParas.filter(p => p.runs.map(r => r.text).join('').trim().length <= 40).length / textParas.length;
    if (avg > 0 && shortParas >= 0.6) lh = (lh || 1) + Math.min(1.5, avg / main.sz / 1.2);
  }
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

/** 직접 쓴 파서로 슬라이드 하나 → { recs, ids, bg (SVG 글), full (SVG 글 | null), text } */
function ownSlide(doc, k, media, tag, outDir, full) {
  let slide;
  try { slide = parseSlide(doc, k); } catch (e) { slide = { w: doc.w, h: doc.h, bg: { type: 'solid', color: '#ffffff', alpha: 1 }, shapes: [], error: e.message }; }
  const embed = part => media.uri(part);
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
  return { recs, ids, bg: slideSvg(slide, { skip, embed }), full: full ? slideSvg(slide, { embed }) : null, text: slideText(slide) };
}

// ---------------- 이음오피스 엔진 경로 ----------------
/** 엔진으로 슬라이드 하나 → ownSlide 와 같은 모양 */
function engineSlide(eng, ctx, k, media, tag, outDir, full) {
  let m = 0;
  const writeElement = svg => { m++; const f = `slide-${tag}-e${String(m).padStart(2, '0')}.svg`; fs.writeFileSync(path.join(outDir, f), svg); return f; };
  const r = eng.slideRecords(ctx, k, media, writeElement);
  const D = require('./pptxdraw');
  const imageSize = media.size.bind(media);
  return { recs: r.recs, ids: r.ids, bg: D.slideSvg(r.tree, { skip: r.skip, imageSize }), full: full ? D.slideSvg(r.tree, { imageSize }) : null, text: r.text };
}

/**
 * 파일 → 캐시 폴더. opts.full = 슬라이드 그림(slide-NNN.svg)도 만든다 (PowerPoint 가 없을 때)
 *   opts.engine === false 면 직접 쓴 파서만 · opts.fonts === false 면 설치된 글꼴 없이 추정 메트릭으로 (점검용) · opts.log(msg) 경고
 * → { n, w, h, slides: ['slide-001.svg', …] | null, texts: [글…], raw, engine: 'ieum' | 'own' | 'mixed' }
 */
async function build(file, outDir, opts = {}) {
  fs.mkdirSync(outDir, { recursive: true });
  const log = typeof opts.log === 'function' ? opts.log : () => {};
  let eng = null, ctx = null;
  if (opts.engine !== false) {
    try { eng = require('./pptxengine'); ctx = await eng.open(file, { fonts: opts.fonts }); } catch (e) { eng = null; ctx = null; log(`이음오피스 엔진이 PPT 를 열지 못해 직접 쓴 파서로 읽습니다: ${e.message}`); }
  }
  let doc = null; // 직접 쓴 파서 (엔진이 없거나 한 슬라이드에서 실패했을 때)
  const own = () => (doc = doc || openPptx(file));
  if (!ctx) own();
  const n = ctx ? ctx.slides.length : doc.slides.length;
  const W = ctx ? ctx.w : r2(doc.w), H = ctx ? ctx.h : r2(doc.h);
  const pkg = ctx ? { has: p => ctx.archive.has(p), raw: p => Buffer.from(ctx.archive.readBytes(p)) } : { has: p => doc.pkg.has(p), raw: p => doc.pkg.raw(p) };
  const media = new Media(pkg, outDir);
  const raw = { v: 4, w: W, h: H, engine: ctx ? 'ieum' : 'own', bgs: [], slides: [], hide: {}, texts: [] };
  const files = [];
  let fell = 0;
  for (let k = 0; k < n; k++) {
    const tag = String(k + 1).padStart(3, '0');
    let s = null;
    if (ctx) {
      try { s = engineSlide(eng, ctx, k, media, tag, outDir, !!opts.full); } catch (e) {
        log(`슬라이드 ${k + 1}: 이음오피스 엔진이 실패해 직접 쓴 파서로 그립니다: ${e.message}`);
        s = null;
      }
    }
    if (!s) {
      own();
      // 직접 쓴 파서의 슬라이드 번호 맞추기 (둘 다 숨긴 슬라이드를 뺀 차례 — 부분 경로로 찾고, 없으면 같은 번호)
      let kk = k;
      if (ctx) { const p = ctx.slides[k].path; const i = doc.slides.findIndex(x => x.part === p); if (i >= 0) kk = i; }
      if (kk >= doc.slides.length) { raw.bgs.push(''); raw.slides.push([]); raw.hide[String(k + 1)] = []; raw.texts.push(''); continue; }
      const ownMedia = new Media({ has: p => doc.pkg.has(p), raw: p => doc.pkg.raw(p) }, outDir);
      ownMedia.names = media.names; ownMedia.sizes = media.sizes; // 같은 그림은 같은 파일 이름
      s = ownSlide(doc, kk, ownMedia, tag, outDir, !!opts.full);
      if (ctx) fell++;
    }
    const bgName = `slide-${tag}-bg.svg`;
    fs.writeFileSync(path.join(outDir, bgName), s.bg);
    if (opts.full) { const name = `slide-${tag}.svg`; fs.writeFileSync(path.join(outDir, name), s.full); files.push(name); }
    raw.bgs.push(bgName);
    raw.slides.push(s.recs);
    raw.hide[String(k + 1)] = s.ids;
    raw.texts.push(s.text);
  }
  if (fell) raw.engine = fell === n ? 'own' : 'mixed';
  fs.writeFileSync(path.join(outDir, 'layers.json'), JSON.stringify(raw));
  return { n, w: W, h: H, slides: opts.full ? files : null, texts: raw.texts, raw, engine: raw.engine };
}

/** 캐시의 SVG 글 안 href="media-N.ext" 를 같은 폴더 파일의 data: 로 바꿔 넣는다 (<img> 로 보이는 SVG 는 바깥 파일을 못 읽는다) */
function inlineSvg(svg, dir, cache) {
  return svg.replace(/href="(media-\d{1,4}\.(png|jpe?g|gif|bmp|webp|svg))"/g, (m, name, ext) => {
    let uri = cache && cache.get(name);
    if (!uri) {
      try { uri = `data:${MIME[ext.toLowerCase()] || 'application/octet-stream'};base64,${fs.readFileSync(path.join(dir, name)).toString('base64')}`; } catch { return m; }
      if (cache) cache.set(name, uri);
    }
    return `href="${uri}"`;
  });
}

module.exports = { build, classify, textRecord, Media, inlineSvg, MIME };
