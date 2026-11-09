'use strict';
// PPT 요소 따로 고치기 · PowerPoint 없이 슬라이드 그리기 — 이음오피스(IeumOffice, Apache-2.0) PPTX 엔진으로 (vendor/ieum-pptx.js)
//   .pptx → 엔진이 레이아웃·마스터·테마 상속을 풀어 슬라이드 모델을 만들고, 글꼴 메트릭(pptxfonts.js)으로 PowerPoint 처럼 줄을 바꾼
//   RenderTree(px 좌표 · 채우기 · 글 조각)를 준다. 여기서는 그 RenderTree 를
//     ① 요소로 나눈다 (글 · 그림 · 단색 도형 · 요소 하나짜리 그림 · 배경에 둘 것) → layers.json 의 요소 기록 (pptxbuild.js 와 같은 모양)
//     ② SVG 로 그린다 (pptxdraw.js: 슬라이드 전체 · 꺼낸 요소를 뺀 배경 · 요소 하나)
//   엔진이 파일을 못 열면 pptxbuild.js 가 직접 쓴 파서(pptxparse.js)로 돌아간다. 슬라이드 하나에서만 엔진이 실패하면 그 슬라이드만 직접 쓴 파서로.
// 글상자의 글: 엔진이 나눈 줄을 그대로 '\n' 으로 넣는다 (자동 줄바꿈도). PowerPoint 와 같은 자리에서 줄이 바뀌고, 편집기에서는 글상자의
//   fit(글자 크기를 줄여 상자에 맞춤)이 있어 글꼴이 달라도 줄이 한 번 더 꺾이지 않는다. 대신 사용자가 상자를 넓혀도 줄이 다시 합쳐지지
//   않는다 — 글을 고칠 때 Enter 를 지우면 된다. (문단만 넣고 브라우저에 맡기면 글꼴이 조금만 달라도 PowerPoint 와 다른 자리에서 꺾인다.)

const fs = require('fs');
const path = require('path');
const E = require('./vendor/ieum-pptx');
const D = require('./pptxdraw');
const F = require('./pptxfonts');

const PX_PT = 0.75; // 96dpi px → pt
const r2 = v => Math.round(v * 100) / 100;
const pt = v => r2(v * PX_PT);
const hiddenSlide = s => /<p:sld\b[^>]*\sshow=(?:"0"|'0'|"false"|'false')/.test(s.bodyPrefix || '');
const ALIGN = { left: 1, center: 2, right: 3, justify: 1 };
const ANCHOR = { top: 1, middle: 3, bottom: 4 };
const SIMPLE_SHAPES = new Set(['rect', 'roundRect', 'ellipse', 'flowChartProcess', 'flowChartConnector']);
const FOOTER_PH = new Set(['sldNum', 'ftr', 'dt']);

/** 파일 열기 → { size, w, h (pt), slides (숨긴 것 뺌), archive, metrics } */
async function open(file, opts = {}) {
  const bytes = new Uint8Array(fs.readFileSync(file));
  const opened = await E.openPptx(bytes);
  const { size } = opened.deck;
  const slides = opened.deck.slides.filter(s => !hiddenSlide(s));
  return { size, w: r2(size.cx / E.EMU_PER_PT), h: r2(size.cy / E.EMU_PER_PT), slides, archive: opened.archive, metrics: F.metricsProvider({ fonts: opts.fonts }) };
}

/** 슬라이드 모델의 요소를 모두 (그룹 안 · SmartArt 미리보기 포함) id → { el, spid, inPassthrough } */
function elementMap(slide) {
  const map = new Map();
  const walk = (els, pass) => {
    for (const el of els || []) {
      let spid = null;
      try { spid = E.elementSpid(el); } catch { spid = null; } // cNvPr id = PowerPoint COM 의 Shape.Id (PS_BG 가 숨길 때 씀)
      if (spid == null && el.nvId != null && /^\d+$/.test(String(el.nvId))) spid = Number(el.nvId); // 그룹 안 요소는 원본 XML 조각이 없고 nvId 에 들어 있다
      map.set(el.id, { el, spid, pass });
      if (el.type === 'group') walk(el.children, pass);
      if (el.type === 'passthrough') walk(el.previewShapes, true);
    }
  };
  walk(slide.elements, false);
  return map;
}

/** 엔진 RenderTree 만들기 (px = 96dpi, scale 1) */
function tree(ctx, k, media) {
  const slide = ctx.slides[k];
  const resolver = ref => media.file(ref) || undefined; // 그림은 data: 대신 캐시 파일 이름 (서버가 보낼 때 data: 로)
  return E.buildRenderSlide(slide, ctx.size, { fitWidthPx: ctx.size.cx / E.EMU_PER_PX_96, metrics: ctx.metrics, media: resolver, slideNo: k + 1 });
}

const hasBody = n => (n.fill && n.fill.kind !== 'none') || !!n.stroke;
const textRuns = n => (n.text ? n.text.lines.flatMap(l => l.runs).filter(r => !r.isBullet && !r.image && r.text && r.text.trim()) : []);
/** 요소를 어떻게 다룰지: skip(보이는 것 없음) · bg(배경에 그대로) · text · pic · shape · element */
function classify(n, info) {
  if (n.decoration || (info && info.pass)) return 'bg'; // 레이아웃·마스터 장식 · SmartArt
  if (info && info.el && FOOTER_PH.has(info.el.placeholder)) return 'bg';
  const b = n.box;
  if ((b.rotationDeg % 360) !== 0 || b.flipH || b.flipV) return 'bg';
  if (n.type === 'table' || n.type === 'chart' || n.type === 'placeholder-chip' || n.type === 'group') return 'bg';
  if (n.type === 'picture') {
    if (!n.dataUrl) return 'skip';
    if (n.media || n.model3d) return 'bg';
    const sr = n.srcRect;
    if (n.clip || n.duotone || n.lum || n.clrChange || n.biLevel != null || n.stroke || n.shadow || n.glow || n.reflection || n.softEdgePx || n.fill || (sr && (sr.l < 0 || sr.t < 0 || sr.r < 0 || sr.b < 0))) return 'element';
    return 'pic';
  }
  // shape · text
  const txt = textRuns(n).length > 0;
  if (n.text && (n.text.vert || (n.text.txWarp && txt))) return 'bg';
  if (n.extrusion) return 'bg';
  if (n.line) return 'element';
  if (!hasBody(n)) return txt ? 'text' : 'skip';
  const prst = n.presetGeometry || 'rect';
  // 단색 사각형·둥근 사각형·타원은 편집기의 도형으로 (색을 바꿀 수 있게). 테마가 주는 옅은 그림자는 버린다 — 기본 테마의 모든 도형에 붙어 있어 이것 때문에 그림으로 두면 색을 못 바꾼다
  if (n.fill.kind === 'solid' && !n.stroke && !n.glow && !n.reflection && !n.fillOverlay && !n.softEdgePx && !n.pathData && !n.fillPathData && !n.strokePathData && !n.polygonPoints && SIMPLE_SHAPES.has(prst)) return 'shape';
  return 'element';
}

/** 줄의 글 (글머리표 뒤에 띄어쓰기 하나 — 엔진은 글머리표와 글을 따로 놓는다) */
function lineText(l) {
  let s = '';
  for (const r of l.runs) { if (r.image) continue; if (r.isBullet) { s += r.text; if (r.text && !/\s$/.test(r.text)) s += ' '; } else s += r.text; }
  return s.replace(/\s+$/, '');
}
const hex6 = c => { const p = D.parseColor(c); return p ? '#' + p.slice(0, 3).map(v => v.toString(16).padStart(2, '0')).join('') : '#000000'; };
const alphaOf = c => { const p = D.parseColor(c); return p ? p[3] : 1; };

/** 글상자 → 글 요소 기록 (대표 글꼴·크기·색 = 가장 긴 글 조각). 줄은 엔진이 나눈 그대로 ('\n') */
function textRecord(n, box, id) {
  const t = n.text;
  const runs = textRuns(n);
  if (!runs.length) return null;
  const main = runs.reduce((a, b) => (b.text.trim().length > a.text.trim().length ? b : a));
  const lines = t.lines.map(l => lineText(l));
  const text = lines.join('\n').replace(/\s+$/, '');
  if (!text.trim()) return null;
  // 줄 간격: 엔진의 줄 높이(간격을 포함) ÷ 글자 크기 → CSS line-height 배수. pptlayers 는 lh × 1.2 를 쓰므로 1.2 로 나눠 둔다
  const first = t.lines.find(l => l.runs.some(r => r.text && r.text.trim())) || t.lines[0];
  const fs0 = Math.max(1, ...first.runs.map(r => r.fontSizePx || 0), main.fontSizePx);
  const lhCss = first.height > 0 ? first.height / fs0 : 1.2;
  return {
    kind: 'text', id, ...box, text,
    size: pt(main.fontSizePx), bold: !!main.bold, italic: !!main.italic, color: hex6(main.color),
    font: main.srcFontFamily || main.fontFamily || '', fontEa: '',
    align: ALIGN[first.align] || 1, anchor: ANCHOR[t.anchor] || 1,
    ml: pt(t.insets.l), mr: pt(t.insets.r), mt: pt(t.insets.t), mb: pt(t.insets.b),
    lh: r2(Math.max(0.5, Math.min(3, lhCss / 1.2))), wrap: t.wrap === false ? 0 : 1,
  };
}

/** 슬라이드 글 (OBS 자막 · 나누기용): 위 → 아래, 표는 '칸 | 칸'. 자동 줄바꿈은 한 줄로 잇고 문단·<a:br/> 만 줄을 바꾼다 */
function slideText(tr, emap) {
  const boxes = [];
  const bodyText = t => {
    const out = [];
    let cur = null, prevTrail = '';
    for (const l of t.lines) {
      const s = lineText(l);
      if (cur === null || l.paraStart) { if (cur !== null) out.push(cur); cur = s; } else cur += prevTrail + s;
      prevTrail = l.trailingText != null ? l.trailingText : l.trailingSpace ? ' ' : '';
      if (l.softBreakAfter != null) { out.push(cur); cur = ''; prevTrail = ''; }
    }
    if (cur !== null) out.push(cur);
    return out.map(s => s.replace(/\s+$/, '')).filter(s => s.trim()).join('\n');
  };
  const walk = (nodes, off) => {
    for (const n of nodes) {
      if (n.decoration) continue;
      const info = emap.get(n.sourceId);
      if (info && (info.pass || FOOTER_PH.has(info.el.placeholder))) continue;
      const x = n.box.x + off.x, y = n.box.y + off.y;
      if (n.type === 'group') { walk(n.children, { x, y }); continue; }
      if (n.type === 'table') {
        const rows = new Map();
        for (const c of n.cells) { const s = c.text ? bodyText(c.text) : ''; if (!rows.has(c.row)) rows.set(c.row, []); rows.get(c.row)[c.col] = s.replace(/\n/g, ' '); }
        const text = [...rows.entries()].sort((a, b) => a[0] - b[0]).map(([, cells]) => cells.filter(Boolean).join(' | ')).filter(Boolean).join('\n');
        if (text) boxes.push({ y, x, text });
        continue;
      }
      if (n.text) { const text = bodyText(n.text); if (text) boxes.push({ y, x, text }); }
    }
  };
  walk(tr.nodes, { x: 0, y: 0 });
  boxes.sort((a, b) => a.y - b.y || a.x - b.x);
  return boxes.map(b => b.text).join('\n');
}

/**
 * 슬라이드 하나 → { recs (요소 기록), ids (PowerPoint 에서 숨길 도형 id), skip (배경에서 뺄 요소 id), text, tree }
 *   writeElement(svgText) → 파일 이름 (요소 하나짜리 그림을 저장하고 이름을 돌려준다)
 */
function slideRecords(ctx, k, media, writeElement) {
  const tr = tree(ctx, k, media);
  const emap = elementMap(ctx.slides[k]);
  const recs = [], ids = [], skip = new Set();
  const imageSize = media.size.bind(media);
  const boxOf = (n, off) => ({ l: pt(n.box.x + off.x), t: pt(n.box.y + off.y), w: pt(Math.max(0.1, n.box.w)), h: pt(Math.max(0.1, n.box.h)) });
  const walk = (nodes, off, inGroup) => {
    for (const n of nodes) {
      const info = emap.get(n.sourceId);
      // 돌리지 않은 그룹은 풀어서 자식을 하나씩 (자식 상자는 그룹 기준 좌표)
      if (n.type === 'group' && !n.decoration && !(info && info.pass) && (n.box.rotationDeg % 360) === 0 && !n.box.flipH && !n.box.flipV) { walk(n.children, { x: off.x + n.box.x, y: off.y + n.box.y }, true); continue; }
      const how = classify(n, info);
      if (how === 'bg') continue;
      if (how === 'skip') { skip.add(n.id); continue; }
      const box = boxOf(n, off);
      let body = null;
      if (how === 'pic') {
        const sr = n.srcRect;
        const crop = sr && [sr.l, sr.t, sr.r, sr.b].some(v => v > 0) ? { l: r2(Math.max(0, sr.l)), t: r2(Math.max(0, sr.t)), r: r2(Math.max(0, sr.r)), b: r2(Math.max(0, sr.b)) } : null;
        body = { kind: 'image', id: n.id, ...box, file: n.dataUrl, fit: 'cover', ...(crop ? { crop } : {}), opacity: n.opacity != null ? r2(n.opacity) : 1 };
      } else if (how === 'shape') {
        const prst = n.presetGeometry || 'rect';
        const radiusPx = prst === 'roundRect' ? (n.cornerRadiusPx != null ? n.cornerRadiusPx : Math.min(n.box.w, n.box.h) * 0.16667) : 0;
        body = { kind: 'shape', id: n.id, ...box, fill: hex6(n.fill.color), ellipse: prst === 'ellipse' || prst === 'flowChartConnector', radius: pt(radiusPx), opacity: r2(alphaOf(n.fill.color)) };
      } else if (how === 'element') {
        const el = D.elementSvg(how === 'element' && n.text ? { ...n, text: undefined } : n, { offset: off, imageSize }); // 글은 따로 글 요소로
        if (el) body = { kind: 'image', id: n.id, l: pt(el.x), t: pt(el.y), w: pt(el.w), h: pt(el.h), file: writeElement(el.svg), fit: 'contain', opacity: 1 };
      }
      const trec = how !== 'pic' ? textRecord(n, box, n.id) : null;
      skip.add(n.id);
      if (!body && !trec) continue;
      if (body) recs.push(body);
      if (trec) recs.push(trec);
      if (info && info.spid != null) ids.push(String(info.spid));
    }
  };
  walk(tr.nodes, { x: 0, y: 0 }, false);
  return { recs, ids, skip, text: slideText(tr, emap), tree: tr };
}

module.exports = { open, tree, slideRecords, classify, textRecord, slideText, elementMap, hiddenSlide };
