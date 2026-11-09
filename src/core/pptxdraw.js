'use strict';
// 이음오피스 엔진의 RenderTree(그릴 준비가 끝난 슬라이드 데이터: px 좌표 · 채우기·선 · 글꼴 메트릭으로 줄바꿈한 글) → SVG
//   이음오피스 apps/slides/src/renderer/export-svg.ts(+ konva-adapter.ts 의 보조 함수, Apache-2.0) 를 Node 용으로 옮겨 고쳐 쓴 것 —
//   캔버스가 없으므로 그림 픽셀 효과(듀오톤 · 밝기/대비 · 흑백)는 SVG 필터로, 자르기는 단위 viewBox 로, 가장자리 흐림은 마스크로 바꿨고
//   WordArt 휨은 휘지 않은 글로 그린다. 글은 엔진이 배치한 줄·글 조각 그대로 <text> 에 놓고 textLength 로 잰 너비에 맞춘다
//   (이 PC 의 글꼴이 달라도 줄바꿈·자리가 PowerPoint 와 같게).
//   그림은 data: 를 넣지 않고 href="media-N.ext"(같은 폴더의 파일 이름) 로 둔다 — 서버가 보내 줄 때 data: 로 바꿔 넣는다 (hymns.inlineSvg).
//   같은 그림은 한 SVG 안에서 <defs> 의 <image> 하나를 <use> 로 다시 쓴다.
// 쓰는 쪽: pptxengine.js (슬라이드 전체 · 꺼낸 요소를 뺀 배경 · 요소 하나짜리 그림)

const SVG_NS = 'http://www.w3.org/2000/svg';
const XLINK = 'http://www.w3.org/1999/xlink';
const CHART_FONT = 'Calibri, Carlito, Arial, sans-serif';

/** 소수 셋째 자리까지 (인쇄 품질에 충분하고 글이 짧다) */
function n(v) {
  if (!Number.isFinite(v)) return '0';
  const r = Math.round(v * 1000) / 1000;
  return r === 0 ? '0' : String(r);
}
const esc = s => String(s).replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');
// eslint-disable-next-line no-control-regex
const XML_INVALID = /[\u0000-\u0008\u000B\u000C\u000E-\u001F￾￿]/g;
const escText = s => esc(String(s).replace(XML_INVALID, ''));
function attrs(a) {
  let out = '';
  for (const [k, v] of Object.entries(a)) {
    if (v === undefined || v === null || v === '') continue;
    out += ` ${k}="${typeof v === 'number' ? n(v) : esc(v)}"`;
  }
  return out;
}
const el = (tag, a, inner = '') => (inner ? `<${tag}${attrs(a)}>${inner}</${tag}>` : `<${tag}${attrs(a)}/>`);

/** 색 정리 (#RRGGBB · #RRGGBBAA → rgba · 이름) */
function normalizeColor(c) {
  if (c == null) return '#000000';
  c = String(c);
  if (c === 'none') return 'transparent';
  if (/^#?[0-9A-Fa-f]{8}$/.test(c)) {
    const h = c.replace(/^#/, '');
    return `rgba(${parseInt(h.slice(0, 2), 16)},${parseInt(h.slice(2, 4), 16)},${parseInt(h.slice(4, 6), 16)},${(parseInt(h.slice(6, 8), 16) / 255).toFixed(3)})`;
  }
  return c.startsWith('#') || c.startsWith('rgb') ? c : `#${c}`;
}
/** rgba(r,g,b,a) → 색 + 투명도 따로 (SVG stop-color 는 알파를 모르는 뷰어가 있다) */
function splitAlpha(color) {
  const c = normalizeColor(color);
  if (c === 'transparent') return { color: '#000000', opacity: 0 };
  const m = /^rgba\((\d+),(\d+),(\d+),([0-9.]+)\)$/.exec(c);
  if (!m) return { color: c };
  return { color: `rgb(${m[1]},${m[2]},${m[3]})`, opacity: Number(m[4]) };
}
/** 색 → [r,g,b,a] (0-255, a 0-1) */
function parseColor(c) {
  const s = normalizeColor(c);
  let m = /^#([0-9a-f]{6})$/i.exec(s);
  if (m) return [parseInt(m[1].slice(0, 2), 16), parseInt(m[1].slice(2, 4), 16), parseInt(m[1].slice(4, 6), 16), 1];
  m = /^rgba?\((\d+),(\d+),(\d+)(?:,([0-9.]+))?\)$/.exec(s);
  if (m) return [+m[1], +m[2], +m[3], m[4] != null ? +m[4] : 1];
  return null;
}
const srgbToLin = v => (v <= 0.04045 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4);
const linToSrgb = v => (v <= 0.0031308 ? v * 12.92 : 1.055 * v ** (1 / 2.4) - 0.055);
/**
 * 그라데이션 색 단계: PowerPoint 는 선형 sRGB 로 섞는데 SVG 는 sRGB 로 섞으므로 단계 사이에 중간 색을 넣어 비슷하게 (이음오피스 linearRampStops)
 * → [pos, color, pos, color, …]
 */
function rampStops(stops) {
  const sorted = [...stops].sort((a, b) => a.pos - b.pos);
  const out = [];
  const SUB = 4;
  for (let i = 0; i < sorted.length; i++) {
    const cur = sorted[i];
    out.push(cur.pos, normalizeColor(cur.color));
    const next = sorted[i + 1];
    if (!next || next.color === cur.color || next.pos - cur.pos < 0.02) continue;
    const a = parseColor(cur.color), b = parseColor(next.color);
    if (!a || !b) continue;
    const la = a.slice(0, 3).map(v => srgbToLin(v / 255)), lb = b.slice(0, 3).map(v => srgbToLin(v / 255));
    for (let k = 1; k < SUB; k++) {
      const t = k / SUB;
      const al = a[3] + (b[3] - a[3]) * t;
      const rgb = la.map((v, j) => Math.round(linToSrgb(v + (lb[j] - v) * t) * 255));
      out.push(cur.pos + (next.pos - cur.pos) * t, al >= 1 ? `rgb(${rgb.join(',')})` : `rgba(${rgb.join(',')},${al.toFixed(3)})`);
    }
  }
  return out;
}
/** a:lin 각도 → 단위 방향 (scaled 면 상자 비율만큼 늘린 각도) */
function gradientDirection(angleDeg, scaled, w, h) {
  const rad = (angleDeg * Math.PI) / 180;
  if (!scaled) return [Math.cos(rad), Math.sin(rad)];
  const vx = h * Math.cos(rad), vy = w * Math.sin(rad);
  const len = Math.hypot(vx, vy) || 1;
  return [vx / len, vy / len];
}
function radialGeometry(w, h, center, tileRect) {
  const cx = (center ? center.x : 0.5) * w, cy = (center ? center.y : 0.5) * h;
  const x0 = Math.min(0, ((tileRect && tileRect.l) || 0) * w), y0 = Math.min(0, ((tileRect && tileRect.t) || 0) * h);
  const x1 = Math.max(w, w - ((tileRect && tileRect.r) || 0) * w), y1 = Math.max(h, h - ((tileRect && tileRect.b) || 0) * h);
  const r = Math.max(Math.hypot(cx - x0, cy - y0), Math.hypot(x1 - cx, cy - y0), Math.hypot(cx - x0, y1 - cy), Math.hypot(x1 - cx, y1 - cy));
  return { cx, cy, r };
}
/** 두 색을 비율로 섞음 (무늬 채우기를 단색으로 대신할 때) */
function mixColor(fg, bg, t) {
  const a = parseColor(fg) || [0, 0, 0, 1], b = parseColor(bg) || [255, 255, 255, 1];
  const c = a.slice(0, 3).map((v, i) => Math.round(v * t + b[i] * (1 - t)));
  return `rgb(${c.join(',')})`;
}
/** 돌리거나 뒤집은 상자: 가운데를 축으로 (Konva 와 같은 방식) */
function boxTransform(box) {
  const sx = box.flipH ? -1 : 1, sy = box.flipV ? -1 : 1;
  if (!box.rotationDeg && sx === 1 && sy === 1) return `translate(${n(box.x)} ${n(box.y)})`;
  return `translate(${n(box.x + box.w / 2)} ${n(box.y + box.h / 2)})` + (box.rotationDeg ? ` rotate(${n(box.rotationDeg)})` : '') + (sx !== 1 || sy !== 1 ? ` scale(${sx} ${sy})` : '') + ` translate(${n(-box.w / 2)} ${n(-box.h / 2)})`;
}
/** 돌린 상자가 부모 좌표에서 차지하는 똑바른 네모 */
function rotatedBounds(box) {
  if (!box.rotationDeg) return { x: box.x, y: box.y, w: box.w, h: box.h };
  const r = (box.rotationDeg * Math.PI) / 180, c = Math.abs(Math.cos(r)), s = Math.abs(Math.sin(r));
  const w = box.w * c + box.h * s, h = box.w * s + box.h * c;
  return { x: box.x + box.w / 2 - w / 2, y: box.y + box.h / 2 - h / 2, w, h };
}

// ---------------- 곡선 · 부채꼴 ----------------
function controlPoints(x0, y0, x1, y1, x2, y2, t) {
  const d01 = Math.hypot(x1 - x0, y1 - y0), d12 = Math.hypot(x2 - x1, y2 - y1);
  const fa = (t * d01) / (d01 + d12), fb = (t * d12) / (d01 + d12);
  return [x1 - fa * (x2 - x0), y1 - fa * (y2 - y0), x1 + fb * (x2 - x0), y1 + fb * (y2 - y0)];
}
function expandPoints(p, t) {
  const out = [];
  for (let i = 2; i < p.length - 2; i += 2) {
    const cp = controlPoints(p[i - 2], p[i - 1], p[i], p[i + 1], p[i + 2], p[i + 3], t);
    if (Number.isNaN(cp[0])) continue;
    out.push(cp[0], cp[1], p[i], p[i + 1], cp[2], cp[3]);
  }
  return out;
}
/** 꺾은선 (tension > 0 이면 부드러운 곡선, Konva Line 과 같은 제어점) */
function polylinePath(points, tension, closed) {
  if (points.length < 4) return '';
  let d = `M${n(points[0])} ${n(points[1])}`;
  if (tension !== 0 && points.length > 4) {
    if (closed) {
      const len = points.length;
      const first = controlPoints(points[len - 2], points[len - 1], points[0], points[1], points[2], points[3], tension);
      const last = controlPoints(points[len - 4], points[len - 3], points[len - 2], points[len - 1], points[0], points[1], tension);
      const tp = [first[2], first[3]].concat(expandPoints(points, tension)).concat([last[0], last[1], points[len - 2], points[len - 1], last[2], last[3], first[0], first[1], points[0], points[1]]);
      for (let i = 0; i + 5 < tp.length; i += 6) d += `C${n(tp[i])} ${n(tp[i + 1])} ${n(tp[i + 2])} ${n(tp[i + 3])} ${n(tp[i + 4])} ${n(tp[i + 5])}`;
    } else {
      const tp = expandPoints(points, tension), len = tp.length;
      d += `Q${n(tp[0])} ${n(tp[1])} ${n(tp[2])} ${n(tp[3])}`;
      for (let i = 4; i + 5 < len; i += 6) d += `C${n(tp[i])} ${n(tp[i + 1])} ${n(tp[i + 2])} ${n(tp[i + 3])} ${n(tp[i + 4])} ${n(tp[i + 5])}`;
      d += `Q${n(tp[len - 2])} ${n(tp[len - 1])} ${n(points[points.length - 2])} ${n(points[points.length - 1])}`;
    }
  } else for (let i = 2; i < points.length; i += 2) d += `L${n(points[i])} ${n(points[i + 1])}`;
  return closed ? d + 'Z' : d;
}
/** 고리 모양 부채꼴 (12시 = -90° · 시계 방향) */
function arcPath(cx, cy, innerR, outerR, startDeg, sweepDeg) {
  const sweep = Math.max(-360, Math.min(360, sweepDeg));
  const full = Math.abs(sweep) >= 360;
  const a0 = (startDeg * Math.PI) / 180, a1 = ((startDeg + (full ? sweep / 2 : sweep)) * Math.PI) / 180, a2 = ((startDeg + sweep) * Math.PI) / 180;
  const pt = (r, a) => `${n(cx + r * Math.cos(a))} ${n(cy + r * Math.sin(a))}`;
  const dir = sweep >= 0 ? 1 : 0, large = !full && Math.abs(sweep) > 180 ? 1 : 0;
  let d = `M${pt(outerR, a0)}`;
  if (full) d += `A${n(outerR)} ${n(outerR)} 0 0 ${dir} ${pt(outerR, a1)}A${n(outerR)} ${n(outerR)} 0 0 ${dir} ${pt(outerR, a2)}`;
  else d += `A${n(outerR)} ${n(outerR)} 0 ${large} ${dir} ${pt(outerR, a2)}`;
  if (innerR > 0) {
    d += `L${pt(innerR, a2)}`;
    if (full) d += `A${n(innerR)} ${n(innerR)} 0 0 ${1 - dir} ${pt(innerR, a1)}A${n(innerR)} ${n(innerR)} 0 0 ${1 - dir} ${pt(innerR, a0)}`;
    else d += `A${n(innerR)} ${n(innerR)} 0 ${large} ${1 - dir} ${pt(innerR, a0)}`;
  } else d += `L${n(cx)} ${n(cy)}`;
  return d + 'Z';
}
function shadeHex(color, f) {
  const c = parseColor(color);
  if (!c) return color;
  return `rgb(${c.slice(0, 3).map(v => Math.round(v * f)).join(',')})`;
}

/** 무늬 채우기 → SVG pattern 안쪽 (몇 가지 줄무늬·격자는 그대로, 나머지 점무늬는 비율만큼 섞은 단색) */
const PATTERN_PCT = { pct5: 0.05, pct10: 0.1, pct20: 0.2, pct25: 0.25, pct30: 0.3, pct40: 0.4, pct50: 0.5, pct60: 0.6, pct70: 0.7, pct75: 0.75, pct80: 0.8, pct90: 0.9 };
function patternBody(preset, fg, bg, s) {
  const L = (x1, y1, x2, y2, w) => el('line', { x1, y1, x2, y2, stroke: fg, 'stroke-width': w });
  const back = el('rect', { width: s, height: s, fill: bg });
  switch (preset) {
    case 'ltUpDiag': case 'upDiag': case 'wdUpDiag': return back + L(0, s, s, 0, preset === 'ltUpDiag' ? 1 : 2);
    case 'ltDnDiag': case 'dkDnDiag': case 'dnDiag': case 'wdDnDiag': return back + L(0, 0, s, s, preset === 'ltDnDiag' ? 1 : 2);
    case 'ltHorz': case 'horz': case 'narHorz': case 'dkHorz': return back + L(0, s / 2, s, s / 2, preset === 'ltHorz' ? 1 : 2);
    case 'ltVert': case 'vert': case 'narVert': case 'dkVert': return back + L(s / 2, 0, s / 2, s, preset === 'ltVert' ? 1 : 2);
    case 'cross': case 'smGrid': case 'lgGrid': return back + L(0, s / 2, s, s / 2, 1) + L(s / 2, 0, s / 2, s, 1);
    case 'diagCross': case 'openDmnd': case 'smCheck': return back + L(0, 0, s, s, 1) + L(0, s, s, 0, 1);
    default: return null;
  }
}

/** 한 SVG 를 만드는 동안의 정의(defs) · 그림 재사용 */
class Page {
  constructor(opts) {
    this.defs = [];
    this.ids = 0;
    this.prefix = opts.idPrefix || '';
    this.imageSize = opts.imageSize || (() => undefined); // href → { w, h } (가로세로 비율이 필요한 바둑판 채우기 · 글머리 그림)
    this.images = new Map(); // href → defs 의 <image id> (한 SVG 안에서 같은 그림은 한 번만)
    this.fonts = new Set();
  }
  id(kind) { return `${this.prefix}${kind}${++this.ids}`; }
  def(m) { this.defs.push(m); }
  defsMarkup() { return this.defs.length ? `<defs>${this.defs.join('')}</defs>` : ''; }
  /** 그림 하나 = <image id width=1 height=1> (단위 상자) → <use> 로 크기·자리를 맞춘다 */
  imageDef(href) {
    if (!href) return null;
    let id = this.images.get(href);
    if (!id) { id = this.id('im'); this.images.set(href, id); this.def(el('image', { id, href, width: 1, height: 1, preserveAspectRatio: 'none' })); }
    return id;
  }
  /** 그림을 (x,y,w,h) 에 늘려 그린다. src = { l,t,r,b } 자른 비율 (0..1, 음수 가능) · a = 더할 속성 */
  image(href, x, y, w, h, src, a = {}) {
    const id = this.imageDef(href);
    if (!id || !(w > 0) || !(h > 0)) return '';
    if (!src || ![src.l, src.t, src.r, src.b].some(v => v)) return el('use', { href: `#${id}`, transform: `translate(${n(x)} ${n(y)}) scale(${n(w)} ${n(h)})`, ...a });
    // 자르기: 보이는 원본 영역 [x0,x1]×[y0,y1] (단위) — 음수(그림보다 틀이 큼)면 그 몫만큼 틀 안쪽에 놓는다 (konva-adapter cropToKonva)
    const x0 = Math.max(src.l, 0), y0 = Math.max(src.t, 0), x1 = 1 - Math.max(src.r, 0), y1 = 1 - Math.max(src.b, 0);
    let dest = { x, y, w, h };
    if (src.l < 0 || src.t < 0 || src.r < 0 || src.b < 0) {
      const spanX = 1 - src.l - src.r, spanY = 1 - src.t - src.b;
      if (spanX > 0 && spanY > 0) dest = { x: x + (w * (x0 - src.l)) / spanX, y: y + (h * (y0 - src.t)) / spanY, w: Math.max((w * (x1 - x0)) / spanX, 1), h: Math.max((h * (y1 - y0)) / spanY, 1) };
    }
    const vw = Math.max(x1 - x0, 1e-4), vh = Math.max(y1 - y0, 1e-4);
    return `<svg${attrs({ x: dest.x, y: dest.y, width: dest.w, height: dest.h, viewBox: `${n(x0)} ${n(y0)} ${n(vw)} ${n(vh)}`, preserveAspectRatio: 'none', overflow: 'hidden', ...a })}>${el('use', { href: `#${id}` })}</svg>`;
  }
  gradientStops(stops) {
    const ramp = rampStops(stops);
    let out = '';
    for (let i = 0; i + 1 < ramp.length; i += 2) { const c = splitAlpha(ramp[i + 1]); out += el('stop', { offset: Math.max(0, Math.min(1, ramp[i])), 'stop-color': c.color, 'stop-opacity': c.opacity }); }
    return out;
  }
  linearGradient(stops, angleDeg, scaled, w, h, o) {
    const [dx, dy] = gradientDirection(angleDeg || 0, scaled, w, h);
    const cx = o.x + w / 2, cy = o.y + h / 2, len = Math.abs(dx) * w + Math.abs(dy) * h;
    const id = this.id('lg');
    this.def(el('linearGradient', { id, gradientUnits: 'userSpaceOnUse', x1: cx - (dx * len) / 2, y1: cy - (dy * len) / 2, x2: cx + (dx * len) / 2, y2: cy + (dy * len) / 2 }, this.gradientStops(stops)));
    return `url(#${id})`;
  }
  /** 그림 픽셀 효과(듀오톤 · 밝기/대비 · 흑백) → 필터 (clrChange 는 못 한다) */
  pixelFilter(fx) {
    if (!fx || (!fx.duotone && !fx.lum && fx.biLevel == null)) return undefined;
    let inner = '';
    if (fx.lum) {
      const k = 1 + (fx.lum.contrast || 0), b = fx.lum.bright || 0;
      const f = type => el(type, { type: 'linear', slope: k, intercept: 0.5 - 0.5 * k + b });
      inner += el('feComponentTransfer', {}, f('feFuncR') + f('feFuncG') + f('feFuncB'));
    }
    if (fx.biLevel != null) {
      const t = Math.max(0, Math.min(1, fx.biLevel));
      inner += el('feColorMatrix', { type: 'saturate', values: 0 });
      const f = type => el(type, { type: 'discrete', tableValues: `${t <= 0 ? '1 1' : t >= 1 ? '0 0' : '0 1'}` });
      // 임계값: 회색(0..1)을 두 단계로 — discrete 표는 등간격이라 임계 0.5 만 정확하고 나머지는 근사
      inner += el('feComponentTransfer', {}, f('feFuncR') + f('feFuncG') + f('feFuncB'));
    }
    if (fx.duotone) {
      const d = parseColor(fx.duotone[0]) || [0, 0, 0, 1], l = parseColor(fx.duotone[1]) || [255, 255, 255, 1];
      inner += el('feColorMatrix', { type: 'saturate', values: 0 });
      const f = (type, i) => el(type, { type: 'table', tableValues: `${n(d[i] / 255)} ${n(l[i] / 255)}` });
      inner += el('feComponentTransfer', {}, f('feFuncR', 0) + f('feFuncG', 1) + f('feFuncB', 2));
    }
    const id = this.id('px');
    this.def(el('filter', { id, 'color-interpolation-filters': 'sRGB' }, inner));
    return `url(#${id})`;
  }
  /** RenderFill → 칠할 것 { paint, opacity, filter } (없으면 null) */
  fillPaint(fill, w, h, o = { x: 0, y: 0 }) {
    if (!fill || fill.kind === 'none') return null;
    switch (fill.kind) {
      case 'solid': return { paint: normalizeColor(fill.color) };
      case 'pattern': {
        const s = Math.max(4, fill.cellPx || 8);
        const body = patternBody(fill.preset, normalizeColor(fill.fg), normalizeColor(fill.bg), s);
        if (!body) return { paint: mixColor(fill.fg, fill.bg, PATTERN_PCT[fill.preset] || 0.5) };
        const id = this.id('pt');
        this.def(el('pattern', { id, patternUnits: 'userSpaceOnUse', x: o.x, y: o.y, width: s, height: s }, body));
        return { paint: `url(#${id})` };
      }
      case 'gradient': {
        if (fill.radial) {
          const { cx, cy, r } = radialGeometry(w, h, fill.center, fill.tileRect);
          const id = this.id('rg');
          this.def(el('radialGradient', { id, gradientUnits: 'userSpaceOnUse', cx: o.x + cx, cy: o.y + cy, r }, this.gradientStops(fill.stops)));
          return { paint: `url(#${id})` };
        }
        return { paint: this.linearGradient(fill.stops, fill.angleDeg, fill.scaled, w, h, o) };
      }
      case 'image': {
        const id = this.imageDef(fill.dataUrl);
        if (!id) return null;
        const pid = this.id('ip');
        const opacity = fill.alpha != null && fill.alpha < 1 ? fill.alpha : undefined;
        let tileW = w, tileH = h, inner;
        if (fill.mode === 'tile') {
          const sz = this.imageSize(fill.dataUrl);
          const t = fill.tile;
          if (sz && sz.w > 0 && sz.h > 0) { tileW = Math.max(1, sz.w * (t ? t.scaleX : 1)); tileH = Math.max(1, sz.h * (t ? t.scaleY : 1)); }
          inner = el('use', { href: `#${id}`, transform: `scale(${n(tileW)} ${n(tileH)})` });
          this.def(el('pattern', { id: pid, patternUnits: 'userSpaceOnUse', x: o.x + (t ? t.txPx : 0), y: o.y + (t ? t.tyPx : 0), width: tileW, height: tileH }, inner));
        } else {
          // 늘려 채우기 (fillRect 안쪽에 맞춤 — 음수면 틀 밖으로 넘친 만큼 잘림)
          const fr = fill.fillRect || { l: 0, t: 0, r: 0, b: 0 };
          const ix = w * fr.l, iy = h * fr.t, iw = Math.max(1, w * (1 - fr.l - fr.r)), ih = Math.max(1, h * (1 - fr.t - fr.b));
          inner = el('use', { href: `#${id}`, transform: `translate(${n(ix)} ${n(iy)}) scale(${n(iw)} ${n(ih)})` });
          this.def(el('pattern', { id: pid, patternUnits: 'userSpaceOnUse', x: o.x, y: o.y, width: w, height: h }, inner));
        }
        return { paint: `url(#${pid})`, opacity, filter: this.pixelFilter(fill) };
      }
      default: return null;
    }
  }
  fillAttrs(p) { return p ? { fill: p.paint, 'fill-opacity': p.opacity, filter: p.filter } : { fill: 'none' }; }
  strokeAttrs(stroke, size, o = { x: 0, y: 0 }) {
    if (!stroke) return {};
    const paint = stroke.gradient && size ? this.linearGradient(stroke.gradient.stops, stroke.gradient.angleDeg, stroke.gradient.scaled, size.w, size.h, o) : normalizeColor(stroke.color);
    return {
      stroke: paint, 'stroke-width': stroke.widthPx,
      ...(stroke.dash && stroke.dash.length ? { 'stroke-dasharray': stroke.dash.map(n).join(' ') } : {}),
      ...(stroke.cap ? { 'stroke-linecap': stroke.cap } : {}), ...(stroke.join ? { 'stroke-linejoin': stroke.join } : {}),
    };
  }
  /** 바깥 그림자·빛 → feDropShadow (안쪽·원근 그림자는 못 그린다). region = 필터가 미치는 영역 (userSpaceOnUse) */
  shadowFilter(shadow, glow, region) {
    const sh = shadow && !(shadow.inner || shadow.scaleX != null || shadow.scaleY != null || shadow.skewXDeg || shadow.skewYDeg) ? shadow : null;
    const s = sh ? { color: sh.color, blur: sh.blurPx, dx: sh.offsetX, dy: sh.offsetY } : glow ? { color: glow.color, blur: glow.blurPx, dx: 0, dy: 0 } : null;
    if (!s) return undefined;
    const c = splitAlpha(s.color);
    const pad = s.blur * 3 + Math.max(Math.abs(s.dx), Math.abs(s.dy)) + 4;
    const id = this.id('sh');
    this.def(el('filter', { id, filterUnits: 'userSpaceOnUse', x: region.x - pad, y: region.y - pad, width: region.w + 2 * pad, height: region.h + 2 * pad }, el('feDropShadow', { dx: s.dx, dy: s.dy, stdDeviation: s.blur / 2, 'flood-color': c.color, 'flood-opacity': c.opacity })));
    return `url(#${id})`;
  }
  blurFilter(blurPx, region) {
    const id = this.id('bl'), pad = blurPx * 3 + 2;
    this.def(el('filter', { id, filterUnits: 'userSpaceOnUse', x: region.x - pad, y: region.y - pad, width: region.w + 2 * pad, height: region.h + 2 * pad }, el('feGaussianBlur', { stdDeviation: blurPx / 2 })));
    return `url(#${id})`;
  }
  clipPath(inner) { const id = this.id('cp'); this.def(el('clipPath', { id }, inner)); return `url(#${id})`; }
  /** 가장자리 흐림(softEdge): 모양을 흐린 흰 마스크 */
  softMask(outline, w, h, radiusPx) {
    const id = this.id('sm'), pad = radiusPx * 2 + 2;
    const f = this.blurFilter(radiusPx, { x: 0, y: 0, w, h });
    this.def(el('mask', { id, maskUnits: 'userSpaceOnUse', x: -pad, y: -pad, width: w + 2 * pad, height: h + 2 * pad }, `<g filter="${f}">${outline({ fill: '#fff' })}</g>`));
    return `url(#${id})`;
  }
}

// ---------------- 모양 ----------------
const presetKind = p => (p === 'ellipse' || p === 'circle' ? 'ellipse' : p === 'roundRect' ? 'roundRect' : 'rect');
/** 닫힌 외곽선 요소 (채우기·선 속성은 a 로) */
function outlineElement(geom, w, h, a) {
  if (geom.pathData) return el('path', { d: geom.pathData, 'stroke-linejoin': 'round', ...a });
  if (geom.polygonPoints) return el('polygon', { points: geom.polygonPoints.map(n).join(' '), 'stroke-linejoin': 'round', ...a });
  if (geom.ellipse) return el('ellipse', { cx: w / 2, cy: h / 2, rx: w / 2, ry: h / 2, ...a });
  const r = Math.min(geom.cornerRadiusPx || 0, Math.min(w, h) / 2);
  return el('rect', { width: w, height: h, rx: r || undefined, ry: r || undefined, ...a });
}
function shapeGeom(shape) {
  if (shape.fillPathData || shape.pathData) return { pathData: shape.fillPathData || shape.pathData };
  if (shape.polygonPoints) return { polygonPoints: shape.polygonPoints };
  const kind = presetKind(shape.presetGeometry);
  if (kind === 'ellipse') return { ellipse: true };
  const rounded = kind === 'roundRect' || shape.cornerRadiusPx != null;
  return { cornerRadiusPx: rounded ? (shape.cornerRadiusPx != null ? shape.cornerRadiusPx : Math.min(shape.box.w, shape.box.h) * 0.167) : 0 };
}

// ---------------- 화살표 ----------------
function arrowHead(end, pts, atStart, color) {
  const nPts = pts.length / 2;
  if (nPts < 2) return '';
  let angle, tipX, tipY;
  if (atStart) { angle = Math.atan2(pts[1] - pts[3], pts[0] - pts[2]) * (180 / Math.PI); tipX = pts[0]; tipY = pts[1]; } else {
    const lx = pts[(nPts - 1) * 2], ly = pts[(nPts - 1) * 2 + 1];
    angle = Math.atan2(ly - pts[(nPts - 2) * 2 + 1], lx - pts[(nPts - 2) * 2]) * (180 / Math.PI); tipX = lx; tipY = ly;
  }
  const w = end.widthPx, l = end.lengthPx, halfW = w / 2;
  const tf = `translate(${n(tipX)} ${n(tipY)}) rotate(${n(angle)})`;
  if (end.type === 'oval') return el('ellipse', { rx: l / 2, ry: halfW, fill: color, transform: tf });
  if (end.type === 'diamond') return el('polygon', { points: [0, 0, -l / 2, -halfW, -l, 0, -l / 2, halfW].map(n).join(' '), fill: color, transform: tf });
  if (end.type === 'stealth') return el('polygon', { points: [0, 0, -l, -halfW, -l * 0.65, 0, -l, halfW].map(n).join(' '), fill: color, transform: tf });
  if (end.type === 'triangle') return el('polygon', { points: [0, 0, -l, -halfW, -l, halfW].map(n).join(' '), fill: color, transform: tf });
  return el('polyline', { points: [-l, -halfW, 0, 0, -l, halfW].map(n).join(' '), fill: 'none', stroke: color, 'stroke-width': Math.max(1, w * 0.12), 'stroke-linecap': 'round', 'stroke-linejoin': 'round', transform: tf });
}
function pointsBounds(points) {
  let x0 = Infinity, y0 = Infinity, x1 = -Infinity, y1 = -Infinity;
  for (let i = 0; i + 1 < points.length; i += 2) { x0 = Math.min(x0, points[i]); x1 = Math.max(x1, points[i]); y0 = Math.min(y0, points[i + 1]); y1 = Math.max(y1, points[i + 1]); }
  if (!Number.isFinite(x0)) return { x: 0, y: 0, w: 0, h: 0 };
  return { x: x0, y: y0, w: x1 - x0, h: y1 - y0 };
}

// ---------------- 글 ----------------
/** 글 조각 하나 → <text> (x0 = 조각 왼쪽, baseY = 기준선 y, 둘 다 SVG 좌표) */
function glyphText(b, run, x0, baseY, extra = {}) {
  const family = run.fontFamily || 'Arial';
  b.fonts.add(family);
  const trailing = (run.letterSpacingPx || 0) + (run.justifyExtraPx || 0);
  const chars = [...run.text];
  // 잰 너비에 맞춤: 이 PC 의 글꼴이 달라도 글자 자리·줄 길이가 PowerPoint 와 같게 (공백만인 조각 · 한 글자는 제외)
  const fitWidth = chars.length >= 2 && run.text.trim() !== '' && run.justifyExtraPx == null ? Math.max(run.widthPx - trailing, 0) : 0;
  const rot = run.rotate90 ? 90 : run.rotate270 ? -90 : 0;
  const a = {
    x: run.rtl ? x0 + run.widthPx : x0, y: baseY,
    'font-family': family, 'font-size': run.fontSizePx,
    'font-weight': run.bold ? 700 : undefined, 'font-style': run.italic ? 'italic' : undefined,
    'letter-spacing': trailing && Math.abs(trailing) > 1e-3 ? trailing : undefined,
    'text-decoration': [run.underline ? 'underline' : '', run.strike ? 'line-through' : ''].filter(Boolean).join(' ') || undefined,
    direction: run.rtl ? 'rtl' : undefined, 'unicode-bidi': run.rtl ? 'bidi-override' : undefined,
    textLength: fitWidth > 0 ? fitWidth : undefined, lengthAdjust: fitWidth > 0 ? 'spacingAndGlyphs' : undefined,
    'xml:space': 'preserve',
    transform: rot ? `rotate(${n(rot)} ${n(x0)} ${n(baseY)})` : undefined,
    ...extra,
  };
  if (run.gradient && !extra.fill) {
    const [gx, gy] = gradientDirection(run.gradient.angleDeg || 0, run.gradient.scaled, run.widthPx, run.fontSizePx);
    const asc = run.ascentPx != null ? run.ascentPx : run.fontSizePx * 0.8;
    const cx = x0 + run.widthPx / 2, cy = baseY - asc + run.fontSizePx * 0.5, len = Math.abs(gx) * run.widthPx + Math.abs(gy) * run.fontSizePx;
    const id = b.id('tg');
    b.def(el('linearGradient', { id, gradientUnits: 'userSpaceOnUse', x1: cx - (gx * len) / 2, y1: cy - (gy * len) / 2, x2: cx + (gx * len) / 2, y2: cy + (gy * len) / 2 }, b.gradientStops(run.gradient.stops)));
    a.fill = `url(#${id})`;
  } else if (!extra.fill) a.fill = normalizeColor(run.color);
  if (run.outline) { a.stroke = normalizeColor(run.outline.color); a['stroke-width'] = Math.max(run.outline.widthPx, 0.5); a['paint-order'] = 'stroke'; a['stroke-linejoin'] = 'round'; }
  const sh = run.shadow || (run.glow ? { color: run.glow.color, blurPx: run.glow.blurPx, offsetX: 0, offsetY: 0 } : null);
  if (sh) a.filter = b.shadowFilter({ color: sh.color, blurPx: sh.blurPx || 0, offsetX: sh.offsetX || 0, offsetY: sh.offsetY || 0 }, undefined, { x: x0, y: baseY - run.fontSizePx, w: run.widthPx, h: run.fontSizePx * 1.5 });
  return el('text', a, escText(run.text));
}
/** 글 몸통(배치된 줄들) → 강조 바탕 · 돌출 · 글 · 반사 */
function textBody(b, text, dx = 0, dy = 0) {
  if (!text) return '';
  const il = dx + text.insets.l, it = dy + text.insets.t;
  let highlights = '', extrusion = '', body = '', reflections = '';
  for (const line of text.lines) {
    for (const run of line.runs) {
      const x0 = il + run.x, baseY = it + run.baselineY;
      if (run.highlight && !text.vert) highlights += el('rect', { x: il + run.x, y: it + line.top, width: run.widthPx, height: line.height, fill: normalizeColor(run.highlight) });
      if (run.image) { // 그림 글머리표
        const h = run.ascentPx != null ? run.ascentPx : run.fontSizePx * 0.8;
        body += b.image(run.image, x0, baseY - h, run.widthPx, h);
        continue;
      }
      if (!run.text) continue;
      if (text.extrusion) for (const k of [1, 0.5]) extrusion += glyphText(b, run, x0 + text.extrusion.dx * k, baseY + text.extrusion.dy * k, { fill: normalizeColor(shadeHex(text.extrusion.color, 0.7)) });
      body += glyphText(b, run, x0, baseY, text.extrusion ? { fill: normalizeColor(shadeHex(text.extrusion.color, 0.35)) } : {});
      if (run.reflection) {
        const asc = run.ascentPx != null ? run.ascentPx : run.fontSizePx * 0.8;
        const yp = baseY - asc + run.fontSizePx * 1.8 + asc; // 1.8em 아래에 뒤집어
        reflections += `<g transform="translate(0 ${n(2 * yp - asc)}) scale(1 -1)" opacity="0.15">${glyphText(b, { ...run, shadow: undefined, glow: undefined, outline: undefined, underline: false, strike: false }, x0, yp)}</g>`;
      }
    }
  }
  return highlights + extrusion + body + reflections;
}

// ---------------- 요소 ----------------
function emitPicture(b, pic) {
  const { box } = pic;
  const region = { x: 0, y: 0, w: box.w, h: box.h };
  const out = [];
  const opacity = pic.opacity != null && pic.opacity < 1 ? pic.opacity : undefined;
  const filter = b.pixelFilter(pic);
  const shadow = b.shadowFilter(pic.shadow, pic.glow, region);
  const clip = pic.clip;
  const geom = clip ? { pathData: clip.pathData, polygonPoints: clip.polygonPoints, cornerRadiusPx: clip.cornerRadiusPx } : null;
  const outline = a => outlineElement(geom || {}, box.w, box.h, a);
  let image = pic.dataUrl ? b.image(pic.dataUrl, 0, 0, box.w, box.h, pic.srcRect, { opacity, filter }) : '';
  if (image && pic.softEdgePx > 0) image = `<g mask="${b.softMask(outline, box.w, box.h, pic.softEdgePx)}">${image}</g>`;
  if (shadow && (image || pic.fill)) out.push(outline({ fill: pic.bgColor || '#ffffff', filter: shadow, opacity })); // 그림자는 모양을 채운 바탕에
  if (pic.fill) out.push(outline(b.fillAttrs(b.fillPaint(pic.fill, box.w, box.h))));
  if (pic.bgColor && image && (pic.opacity == null || pic.opacity >= 1)) out.push(outline({ fill: pic.bgColor }));
  if (clip && image) out.push(`<g clip-path="${b.clipPath(outline({}))}">${image}</g>`);
  else out.push(image);
  if (pic.stroke) out.push(outline({ fill: 'none', ...b.strokeAttrs(pic.stroke) }));
  return out.join('');
}

function emitTable(b, table) {
  const { box } = table;
  const tblW = table.gridX[table.gridX.length - 1] != null ? table.gridX[table.gridX.length - 1] : box.w;
  const tblH = table.gridY[table.gridY.length - 1] != null ? table.gridY[table.gridY.length - 1] : box.h;
  const out = [];
  if (table.bgFill) out.push(el('rect', { width: tblW, height: tblH, ...b.fillAttrs(b.fillPaint(table.bgFill, tblW, tblH)) }));
  const line = (x1, y1, x2, y2, s) => (s ? el('line', { x1, y1, x2, y2, ...b.strokeAttrs(s) }) : '');
  for (const cell of table.cells) {
    const bw = cell.bevel ? cell.bevel.widthPx || 0 : 0;
    out.push(el('rect', { x: cell.x + bw, y: cell.y + bw, width: Math.max(0, cell.w - 2 * bw), height: Math.max(0, cell.h - 2 * bw), ...b.fillAttrs(b.fillPaint(cell.fill, cell.w, cell.h, { x: cell.x, y: cell.y })) }));
    const bd = cell.borders || {};
    out.push(line(cell.x, cell.y, cell.x + cell.w, cell.y, bd.t), line(cell.x, cell.y + cell.h, cell.x + cell.w, cell.y + cell.h, bd.b), line(cell.x, cell.y, cell.x, cell.y + cell.h, bd.l), line(cell.x + cell.w, cell.y, cell.x + cell.w, cell.y + cell.h, bd.r));
    out.push(textBody(b, cell.text, cell.x, cell.y));
  }
  return out.join('');
}

function emitChart(b, chart) {
  const out = [];
  const { box } = chart;
  if (chart.bgFill) out.push(el('rect', { width: box.w, height: box.h, ...b.fillAttrs(b.fillPaint(chart.bgFill, box.w, box.h)) }));
  if (chart.plotRect) {
    const p = chart.plotRect;
    out.push(el('rect', { x: p.x, y: p.y, width: p.w, height: p.h, ...(p.fill ? b.fillAttrs(b.fillPaint(p.fill, p.w, p.h, { x: p.x, y: p.y })) : { fill: 'none' }), ...(p.borderColor ? { stroke: p.borderColor, 'stroke-width': p.borderWidthPx != null ? p.borderWidthPx : 1 } : {}) }));
  }
  for (const wd of chart.wedges || []) out.push(el('path', { d: arcPath(wd.cx, wd.cy, wd.innerR, wd.outerR, wd.startDeg, wd.sweepDeg), fill: wd.noFill ? 'none' : wd.color, ...(wd.strokeWidthPx === 0 ? {} : { stroke: wd.stroke || '#ffffff', 'stroke-width': wd.strokeWidthPx != null ? wd.strokeWidthPx : 1 }) }));
  for (const g of chart.gridLines || []) out.push(el('line', { x1: g.x1, y1: g.y1, x2: g.x2, y2: g.y2, stroke: g.color, 'stroke-width': g.widthPx != null ? g.widthPx : 1, 'stroke-dasharray': g.dash && g.dash.length ? g.dash.map(n).join(' ') : undefined }));
  for (const a of chart.axisLines || []) out.push(el('line', { x1: a.x1, y1: a.y1, x2: a.x2, y2: a.y2, stroke: a.color, 'stroke-width': a.widthPx }));
  for (const p of chart.paths || []) out.push(el('path', { d: p.d, fill: p.fill, transform: p.dy ? `translate(0 ${n(p.dy)})` : undefined, ...(p.stroke ? { stroke: p.stroke, 'stroke-width': p.strokeWidthPx != null ? p.strokeWidthPx : 1 } : {}) }));
  for (const bar of chart.bars || []) out.push(el('rect', { x: bar.x, y: bar.y, width: bar.w, height: bar.h, ...(bar.fill ? b.fillAttrs(b.fillPaint(bar.fill, bar.w, bar.h, { x: bar.x, y: bar.y })) : { fill: bar.color }) }));
  for (const p of chart.polylines || []) out.push(el('path', { d: polylinePath(p.points, p.smooth ? 0.4 : 0, !!p.closed), fill: p.fill || 'none', stroke: p.color, 'stroke-width': p.widthPx, 'stroke-linecap': 'round', 'stroke-linejoin': 'round', 'stroke-dasharray': p.dash && p.dash.length ? p.dash.map(n).join(' ') : undefined }));
  for (const m of chart.markers || []) out.push(el('circle', { cx: m.x, cy: m.y, r: m.r, fill: m.color }));
  for (const s of chart.swatches || []) out.push(el('rect', { x: s.x, y: s.y, width: s.w, height: s.h, fill: s.color, rx: 1, ry: 1 }));
  for (const l of chart.labels || []) {
    const family = l.fontFamily || CHART_FONT;
    b.fonts.add(family);
    if (l.fill || l.stroke) out.push(el('rect', { x: l.x - l.fontSizePx * 0.3, y: l.y - l.fontSizePx * 0.1, width: (l.w || 0) + l.fontSizePx * 0.6, height: l.fontSizePx * 1.35, fill: l.fill || 'none', ...(l.stroke ? { stroke: l.stroke, 'stroke-width': 1 } : {}) }));
    out.push(el('text', { x: l.x, y: l.y + l.fontSizePx / 2, 'dominant-baseline': 'central', 'font-family': family, 'font-size': l.fontSizePx, 'font-weight': l.bold ? 700 : undefined, 'font-style': l.italic ? 'italic' : undefined, 'letter-spacing': l.letterSpacingPx || undefined, fill: l.color, 'xml:space': 'preserve', transform: l.rotationDeg ? `rotate(${n(l.rotationDeg)} ${n(l.x)} ${n(l.y)})` : undefined }, escText(l.text)));
  }
  if (chart.border) out.push(el('rect', { x: chart.border.widthPx / 2, y: chart.border.widthPx / 2, width: box.w - chart.border.widthPx, height: box.h - chart.border.widthPx, fill: 'none', stroke: chart.border.color, 'stroke-width': chart.border.widthPx }));
  return out.join('');
}

function emitLine(b, shape) {
  const line = shape.line;
  const stroke = b.strokeAttrs(shape.stroke, { w: shape.box.w, h: shape.box.h });
  const color = stroke.stroke || '#000000';
  const sw = stroke['stroke-width'] != null ? stroke['stroke-width'] : 1;
  const { headEnd, tailEnd, bezier, points } = line;
  const filter = b.shadowFilter(shape.shadow, shape.glow, pointsBounds(points));
  const common = { fill: 'none', stroke: color, 'stroke-width': sw, 'stroke-dasharray': stroke['stroke-dasharray'], 'stroke-linecap': stroke['stroke-linecap'] || 'round', 'stroke-linejoin': 'round', filter };
  const out = [];
  if (bezier && bezier.length) {
    let d = `M${n(points[0] || 0)} ${n(points[1] || 0)}`;
    for (let i = 0; i + 5 < bezier.length; i += 6) d += `C${n(bezier[i])} ${n(bezier[i + 1])} ${n(bezier[i + 2])} ${n(bezier[i + 3])} ${n(bezier[i + 4])} ${n(bezier[i + 5])}`;
    out.push(el('path', { d, ...common }));
  } else out.push(el('polyline', { points: points.map(n).join(' '), ...common }));
  if (headEnd) out.push(arrowHead(headEnd, points, true, color));
  if (tailEnd) out.push(arrowHead(tailEnd, points, false, color));
  return out.join('');
}

function emitShape(b, shape, flip) {
  const { box } = shape;
  if (shape.line) return emitLine(b, shape);
  const region = { x: 0, y: 0, w: box.w, h: box.h };
  const fillA = b.fillAttrs(b.fillPaint(shape.fill, box.w, box.h));
  const strokeA = b.strokeAttrs(shape.stroke, { w: box.w, h: box.h });
  const filter = b.shadowFilter(shape.shadow, shape.glow, region);
  const geom = shapeGeom(shape);
  const out = [];
  let geometry = '';
  if (shape.extrusion && shape.extrusion.faces) { // 3D 돌출: 미리 투영된 면들
    for (const f of shape.extrusion.faces) geometry += el('path', { d: f.path, ...(f.front ? fillA : f.color === 'transparent' ? { fill: 'none' } : { fill: normalizeColor(f.color) }), ...(f.stroke ? { stroke: normalizeColor(f.stroke), 'stroke-width': f.strokeWidthPx != null ? f.strokeWidthPx : 1 } : {}), 'stroke-linejoin': 'round' });
  } else if (shape.pathData || shape.fillPathData || shape.strokePathData) {
    if (shape.fillPathData) geometry += el('path', { d: shape.fillPathData, ...fillA, filter });
    if (shape.pathData) geometry += el('path', { d: shape.pathData, ...fillA, ...strokeA, 'stroke-linejoin': strokeA['stroke-linejoin'] || 'round', filter });
    if (shape.strokePathData) geometry += el('path', { d: shape.strokePathData, fill: 'none', ...strokeA, 'stroke-linejoin': strokeA['stroke-linejoin'] || 'round' });
  } else geometry += outlineElement(geom, box.w, box.h, { ...fillA, ...strokeA, filter });
  if (shape.softEdgePx > 0 && box.w >= 1 && box.h >= 1) geometry = `<g mask="${b.softMask(a => outlineElement(geom, box.w, box.h, a), box.w, box.h, shape.softEdgePx)}">${geometry}</g>`;
  // fillOverlay: 흰 바탕 + 본 채우기 + 곱하기 덧칠
  if (shape.fillOverlay && !shape.extrusion) {
    out.push(outlineElement(geom, box.w, box.h, { fill: '#ffffff' }), geometry, outlineElement(geom, box.w, box.h, { ...b.fillAttrs(b.fillPaint(shape.fillOverlay, box.w, box.h)), style: 'mix-blend-mode:multiply' }));
  } else out.push(geometry);
  const text = textBody(b, shape.text);
  if (text) { // PowerPoint 는 모양만 뒤집고 글은 바로 둔다
    const fh = !!box.flipH !== flip.h, fv = !!box.flipV !== flip.v;
    out.push(fh || fv ? `<g transform="translate(${n(fh ? box.w : 0)} ${n(fv ? box.h : 0)}) scale(${fh ? -1 : 1} ${fv ? -1 : 1})">${text}</g>` : text);
  }
  return out.join('');
}

/** 요소 안쪽 (요소 좌표계) — 바깥에서 상자 변환으로 감싼다. skip = 그리지 않을 요소 id */
function nodeBody(b, node, flip, skip) {
  const refl = node.type === 'shape' || node.type === 'text' || node.type === 'picture' ? node.reflection : undefined;
  if (refl) {
    const clone = { ...node, reflection: undefined };
    return reflectionCopy(b, clone, refl, flip, skip) + nodeBody(b, clone, flip, skip);
  }
  switch (node.type) {
    case 'group': {
      const childFlip = { h: !!node.box.flipH !== flip.h, v: !!node.box.flipV !== flip.v };
      return node.children.filter(c => !skip || !skip.has(c.id)).map(c => staticNode(b, c, childFlip, skip)).join('');
    }
    case 'picture': return emitPicture(b, node);
    case 'table': return emitTable(b, node);
    case 'chart': return emitChart(b, node);
    case 'placeholder-chip': return ''; // 못 그리는 것(지원 안 하는 차트 · 미리보기 없는 OLE)의 자리표시 — 예배 화면에는 안 그린다
    default: return emitShape(b, node, flip);
  }
}
/** 반사: 아래에 뒤집어 흐려지는 사본 */
function reflectionCopy(b, node, refl, flip, skip) {
  const { w, h } = node.box;
  if (w < 1 || h < 1) return '';
  const pad = Math.ceil(refl.blurPx) + 2;
  const gradId = b.id('rf'), maskId = b.id('rm');
  b.def(el('linearGradient', { id: gradId, gradientUnits: 'userSpaceOnUse', x1: 0, y1: h, x2: 0, y2: h * (1 - Math.max(refl.endPos, 0.02)) }, el('stop', { offset: 0, 'stop-color': '#fff' }) + el('stop', { offset: 1, 'stop-color': '#000' })));
  b.def(el('mask', { id: maskId, maskUnits: 'userSpaceOnUse', x: -pad, y: -pad, width: w + 2 * pad, height: h + 2 * pad }, el('rect', { x: -pad, y: -pad, width: w + 2 * pad, height: h + 2 * pad, fill: `url(#${gradId})` })));
  const filter = refl.blurPx ? b.blurFilter(refl.blurPx, { x: 0, y: 0, w, h }) : undefined;
  return `<g${attrs({ transform: `translate(0 ${n(2 * h + refl.distPx)}) scale(1 -1)`, opacity: refl.startAlpha, mask: `url(#${maskId})`, filter })}>${nodeBody(b, node, flip, skip)}</g>`;
}
function staticNode(b, node, flip, skip) {
  const body = nodeBody(b, node, flip, skip);
  return body ? `<g transform="${boxTransform(node.box)}">${body}</g>` : '';
}

/**
 * 슬라이드 전체 → SVG 글. opts.skip = 그리지 않을 요소 id 집합(꺼낸 요소) · opts.imageSize(href) · opts.background === false 면 배경 채우기를 뺀다
 */
function slideSvg(tree, opts = {}) {
  const b = new Page(opts);
  const skip = opts.skip && opts.skip.size ? opts.skip : null;
  let body = '';
  if (opts.background !== false) {
    const bg = b.fillPaint(tree.background, tree.widthPx, tree.heightPx);
    body += el('rect', { width: tree.widthPx, height: tree.heightPx, ...(bg ? b.fillAttrs(bg) : { fill: '#ffffff' }) });
  }
  for (const node of tree.nodes) {
    if (skip && skip.has(node.id)) continue;
    try { body += staticNode(b, node, { h: false, v: false }, skip); } catch (e) { if (opts.onError) opts.onError(e, node); }
  }
  return `<svg xmlns="${SVG_NS}" xmlns:xlink="${XLINK}" viewBox="0 0 ${n(tree.widthPx)} ${n(tree.heightPx)}" width="${n(tree.widthPx)}" height="${n(tree.heightPx)}">${b.defsMarkup()}${body}</svg>`;
}

/**
 * 요소 하나 → SVG 글 + 슬라이드 좌표의 자리 { svg, x, y, w, h } (px). 선 두께·그림자만큼 넓힌다.
 * offset = 그룹 안 요소면 그룹의 자리 (요소 상자는 그룹 기준이라 더한다)
 */
function elementSvg(node, opts = {}) {
  const b = new Page(opts);
  const off = opts.offset || { x: 0, y: 0 };
  const box = { ...node.box, x: node.box.x + off.x, y: node.box.y + off.y };
  const bb = rotatedBounds(box);
  let pad = 2;
  const sw = node.stroke ? node.stroke.widthPx || 0 : 0;
  pad += sw;
  if (node.line) { const hb = pointsBounds(node.line.points); pad += Math.max(0, hb.w === 0 || hb.h === 0 ? sw * 2 : 0); for (const e of [node.line.headEnd, node.line.tailEnd]) if (e) pad = Math.max(pad, e.widthPx, e.lengthPx); }
  const sh = node.shadow || node.glow;
  if (sh) pad += (sh.blurPx || 0) * 3 + Math.max(Math.abs(sh.offsetX || 0), Math.abs(sh.offsetY || 0));
  if (node.reflection) pad += box.h + (node.reflection.distPx || 0);
  if (node.softEdgePx) pad += node.softEdgePx * 2;
  const x = bb.x - pad, y = bb.y - pad, w = bb.w + 2 * pad, h = bb.h + 2 * pad;
  let body = '';
  try { body = staticNode(b, { ...node, box }, { h: false, v: false }, null); } catch (e) { if (opts.onError) opts.onError(e, node); return null; }
  if (!body) return null;
  const svg = `<svg xmlns="${SVG_NS}" xmlns:xlink="${XLINK}" viewBox="${n(x)} ${n(y)} ${n(w)} ${n(h)}" width="${n(w)}" height="${n(h)}">${b.defsMarkup()}${body}</svg>`;
  return { svg, x, y, w, h };
}

module.exports = { slideSvg, elementSvg, normalizeColor, parseColor, rotatedBounds, polylinePath, arcPath, rampStops, Page };
