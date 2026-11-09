'use strict';
// pptxparse.js 가 읽은 슬라이드·도형 → SVG 그림 (PowerPoint 없이 슬라이드 그림 · 요소를 뺀 배경 그림 · 요소 하나짜리 그림)
//   - 좌표는 포인트 그대로 (viewBox = 슬라이드 크기), 글은 <foreignObject> 안의 HTML 로 (줄바꿈·정렬·여백을 브라우저가 맞춘다)
//   - 그림(ppt/media)은 data: 주소로 안에 넣는다 (<img> 로 여는 SVG 는 바깥 파일을 못 읽는다)
//   - 미리 정의된 모양(prstGeom)은 자주 쓰는 것만 직접 그리고, 모르는 모양은 사각형으로
//   - 표는 HTML 표로, SmartArt 는 그려 둔 도형으로, 차트·OLE 는 미리 보기 그림이 있으면 그것만

const esc = s => String(s ?? '').replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');
const r2 = n => Math.round(n * 100) / 100;
const HANGUL = /[ㄱ-ㆎ가-힣]/;
const cssFont = n => String(n || '').replace(/["'\\;]/g, '').trim();

// ---------------- 미리 정의된 모양 → 경로 (가로 w · 세로 h 포인트, 원점 왼쪽 위) ----------------
const A = (adj, name, d) => (adj && Number.isFinite(adj[name]) ? adj[name] : d) / 100000;
function polygon(pts) { return 'M' + pts.map(p => `${r2(p[0])} ${r2(p[1])}`).join('L') + 'Z'; }
function regular(n, w, h, rot = -Math.PI / 2) {
  const pts = [];
  for (let k = 0; k < n; k++) { const a = rot + k * 2 * Math.PI / n; pts.push([w / 2 + w / 2 * Math.cos(a), h / 2 + h / 2 * Math.sin(a)]); }
  return polygon(pts);
}
function star(n, w, h, inner) {
  const pts = [];
  for (let k = 0; k < n * 2; k++) { const a = -Math.PI / 2 + k * Math.PI / n; const r = k % 2 ? inner : 1; pts.push([w / 2 + w / 2 * r * Math.cos(a), h / 2 + h / 2 * r * Math.sin(a)]); }
  return polygon(pts);
}
function roundRectPath(w, h, r) {
  r = Math.max(0, Math.min(r, w / 2, h / 2));
  if (!r) return `M0 0H${r2(w)}V${r2(h)}H0Z`;
  return `M${r2(r)} 0H${r2(w - r)}A${r2(r)} ${r2(r)} 0 0 1 ${r2(w)} ${r2(r)}V${r2(h - r)}A${r2(r)} ${r2(r)} 0 0 1 ${r2(w - r)} ${r2(h)}H${r2(r)}A${r2(r)} ${r2(r)} 0 0 1 0 ${r2(h - r)}V${r2(r)}A${r2(r)} ${r2(r)} 0 0 1 ${r2(r)} 0Z`;
}
function ellipsePath(w, h) { const rx = w / 2, ry = h / 2; return `M${r2(rx)} 0A${r2(rx)} ${r2(ry)} 0 1 1 ${r2(rx)} ${r2(h)}A${r2(rx)} ${r2(ry)} 0 1 1 ${r2(rx)} 0Z`; }
/** 화살표 (오른쪽 기준) 점 목록 → 방향 바꾸기 */
function arrowPts(w, h, adj, dir) {
  // 세로 화살표는 가로·세로를 바꿔 그린 뒤 되돌린다
  const vert = dir === 'up' || dir === 'down';
  const W = vert ? h : w, H = vert ? w : h, ss = Math.min(w, h);
  const half = H * A(adj, 'adj1', 50000) / 2, head = Math.min(W, ss * A(adj, 'adj2', 50000));
  let pts = [[0, H / 2 - half], [W - head, H / 2 - half], [W - head, 0], [W, H / 2], [W - head, H], [W - head, H / 2 + half], [0, H / 2 + half]];
  if (dir === 'left' || dir === 'up') pts = pts.map(([x, y]) => [W - x, y]);
  if (vert) pts = pts.map(([x, y]) => [y, x]);
  return polygon(pts);
}
function doubleArrow(w, h, adj, vert) {
  const W = vert ? h : w, H = vert ? w : h, ss = Math.min(w, h);
  const half = H * A(adj, 'adj1', 50000) / 2, head = Math.min(W / 2, ss * A(adj, 'adj2', 50000));
  let pts = [[0, H / 2], [head, 0], [head, H / 2 - half], [W - head, H / 2 - half], [W - head, 0], [W, H / 2], [W - head, H], [W - head, H / 2 + half], [head, H / 2 + half], [head, H]];
  if (vert) pts = pts.map(([x, y]) => [y, x]);
  return polygon(pts);
}
/** 모양 이름 → { d: 경로, open: 선만(채우기 없음) } · 모르는 모양이면 null */
function presetPath(prst, w, h, adj) {
  const ss = Math.min(w, h);
  switch (prst) {
    case 'rect': case 'flowChartProcess': case 'wedgeRectCallout': case 'plaque': case 'cube': case 'can': case 'bevel': case 'frame': case 'actionButtonBlank': case 'flowChartPredefinedProcess': case 'flowChartInternalStorage': case 'flowChartDocument': case 'flowChartMultidocument': case 'flowChartManualInput': case 'flowChartCard': case 'flowChartPunchedTape':
      return { d: `M0 0H${r2(w)}V${r2(h)}H0Z` };
    case 'roundRect': case 'wedgeRoundRectCallout': case 'flowChartAlternateProcess': case 'round2SameRect': case 'round2DiagRect': case 'round1Rect': case 'snipRoundRect':
      return { d: roundRectPath(w, h, ss * A(adj, 'adj', 16667)) };
    case 'flowChartTerminator': return { d: roundRectPath(w, h, h / 2) };
    case 'snip1Rect': case 'snip2SameRect': case 'snip2DiagRect': return { d: `M0 0H${r2(w)}V${r2(h)}H0Z` };
    case 'ellipse': case 'flowChartConnector': case 'wedgeEllipseCallout': case 'cloud': case 'cloudCallout': case 'sun': case 'moon': case 'smileyFace': case 'donut': case 'pie': case 'blockArc': case 'noSmoking': case 'flowChartOr': case 'flowChartSummingJunction':
      return { d: ellipsePath(w, h) };
    case 'arc': return { d: `M${r2(w)} ${r2(h / 2)}A${r2(w / 2)} ${r2(h / 2)} 0 0 0 ${r2(w / 2)} 0`, open: true };
    case 'triangle': case 'flowChartExtract': return { d: polygon([[w * A(adj, 'adj', 50000), 0], [w, h], [0, h]]) };
    case 'flowChartMerge': return { d: polygon([[0, 0], [w, 0], [w / 2, h]]) };
    case 'rtTriangle': return { d: polygon([[0, 0], [w, h], [0, h]]) };
    case 'diamond': case 'flowChartDecision': return { d: polygon([[w / 2, 0], [w, h / 2], [w / 2, h], [0, h / 2]]) };
    case 'parallelogram': case 'flowChartData': { const a = ss * A(adj, 'adj', 25000); return { d: polygon([[a, 0], [w, 0], [w - a, h], [0, h]]) }; }
    case 'trapezoid': case 'flowChartManualOperation': { const a = ss * A(adj, 'adj', 25000); return { d: polygon([[a, 0], [w - a, 0], [w, h], [0, h]]) }; }
    case 'nonIsoscelesTrapezoid': { const a = ss * A(adj, 'adj1', 25000), b = ss * A(adj, 'adj2', 25000); return { d: polygon([[a, 0], [w - b, 0], [w, h], [0, h]]) }; }
    case 'pentagon': return { d: polygon([[w / 2, 0], [w, h * 0.382], [w * 0.809, h], [w * 0.191, h], [0, h * 0.382]]) };
    case 'hexagon': case 'flowChartPreparation': { const a = ss * A(adj, 'adj', 25000); return { d: polygon([[a, 0], [w - a, 0], [w, h / 2], [w - a, h], [a, h], [0, h / 2]]) }; }
    case 'heptagon': return { d: regular(7, w, h) };
    case 'octagon': { const a = ss * A(adj, 'adj', 29289); return { d: polygon([[a, 0], [w - a, 0], [w, a], [w, h - a], [w - a, h], [a, h], [0, h - a], [0, a]]) }; }
    case 'decagon': return { d: regular(10, w, h) };
    case 'dodecagon': return { d: regular(12, w, h) };
    case 'star4': return { d: star(4, w, h, A(adj, 'adj', 12500) * 2) };
    case 'star5': return { d: star(5, w, h, A(adj, 'adj', 19098) * 2) };
    case 'star6': return { d: star(6, w, h, A(adj, 'adj', 28868) * 2) };
    case 'star7': return { d: star(7, w, h, A(adj, 'adj', 34601) * 2) };
    case 'star8': return { d: star(8, w, h, A(adj, 'adj', 37500) * 2) };
    case 'star10': return { d: star(10, w, h, A(adj, 'adj', 42533) * 2) };
    case 'star12': return { d: star(12, w, h, A(adj, 'adj', 37500) * 2) };
    case 'star16': return { d: star(16, w, h, A(adj, 'adj', 37500) * 2) };
    case 'star24': return { d: star(24, w, h, A(adj, 'adj', 37500) * 2) };
    case 'star32': return { d: star(32, w, h, A(adj, 'adj', 37500) * 2) };
    case 'irregularSeal1': case 'irregularSeal2': return { d: star(12, w, h, 0.6) };
    case 'rightArrow': case 'notchedRightArrow': case 'stripedRightArrow': return { d: arrowPts(w, h, adj, 'right') };
    case 'leftArrow': return { d: arrowPts(w, h, adj, 'left') };
    case 'upArrow': return { d: arrowPts(w, h, adj, 'up') };
    case 'downArrow': return { d: arrowPts(w, h, adj, 'down') };
    case 'leftRightArrow': return { d: doubleArrow(w, h, adj, false) };
    case 'upDownArrow': return { d: doubleArrow(w, h, adj, true) };
    case 'homePlate': { const a = Math.min(w, ss * A(adj, 'adj', 50000)); return { d: polygon([[0, 0], [w - a, 0], [w, h / 2], [w - a, h], [0, h]]) }; }
    case 'chevron': { const a = Math.min(w, ss * A(adj, 'adj', 50000)); return { d: polygon([[0, 0], [w - a, 0], [w, h / 2], [w - a, h], [0, h], [a, h / 2]]) }; }
    case 'plus': case 'mathPlus': case 'flowChartSummingJunction2': { const a = ss * A(adj, 'adj', 25000); return { d: polygon([[a, 0], [w - a, 0], [w - a, a], [w, a], [w, h - a], [w - a, h - a], [w - a, h], [a, h], [a, h - a], [0, h - a], [0, a], [a, a]]) }; }
    case 'heart': return { d: `M${r2(w / 2)} ${r2(h)}C${r2(-w * 0.1)} ${r2(h * 0.55)} ${r2(w * 0.05)} ${r2(-h * 0.1)} ${r2(w / 2)} ${r2(h * 0.25)}C${r2(w * 0.95)} ${r2(-h * 0.1)} ${r2(w * 1.1)} ${r2(h * 0.55)} ${r2(w / 2)} ${r2(h)}Z` };
    case 'teardrop': return { d: roundRectPath(w, h, ss / 2) };
    case 'line': case 'straightConnector1': return { d: `M0 0L${r2(w)} ${r2(h)}`, open: true };
    case 'bentConnector2': return { d: `M0 0L${r2(w)} 0L${r2(w)} ${r2(h)}`, open: true };
    case 'bentConnector3': case 'bentConnector4': case 'bentConnector5': return { d: `M0 0L${r2(w / 2)} 0L${r2(w / 2)} ${r2(h)}L${r2(w)} ${r2(h)}`, open: true };
    case 'curvedConnector2': return { d: `M0 0Q${r2(w)} 0 ${r2(w)} ${r2(h)}`, open: true };
    case 'curvedConnector3': case 'curvedConnector4': case 'curvedConnector5': return { d: `M0 0C${r2(w / 2)} 0 ${r2(w / 2)} ${r2(h)} ${r2(w)} ${r2(h)}`, open: true };
    case 'leftBracket': return { d: `M${r2(w)} 0Q0 0 0 ${r2(ss * 0.1)}V${r2(h - ss * 0.1)}Q0 ${r2(h)} ${r2(w)} ${r2(h)}`, open: true };
    case 'rightBracket': return { d: `M0 0Q${r2(w)} 0 ${r2(w)} ${r2(ss * 0.1)}V${r2(h - ss * 0.1)}Q${r2(w)} ${r2(h)} 0 ${r2(h)}`, open: true };
    case 'leftBrace': return { d: `M${r2(w)} 0Q${r2(w / 2)} 0 ${r2(w / 2)} ${r2(h * 0.1)}V${r2(h / 2 - h * 0.05)}L0 ${r2(h / 2)}L${r2(w / 2)} ${r2(h / 2 + h * 0.05)}V${r2(h * 0.9)}Q${r2(w / 2)} ${r2(h)} ${r2(w)} ${r2(h)}`, open: true };
    case 'rightBrace': return { d: `M0 0Q${r2(w / 2)} 0 ${r2(w / 2)} ${r2(h * 0.1)}V${r2(h / 2 - h * 0.05)}L${r2(w)} ${r2(h / 2)}L${r2(w / 2)} ${r2(h / 2 + h * 0.05)}V${r2(h * 0.9)}Q${r2(w / 2)} ${r2(h)} 0 ${r2(h)}`, open: true };
    case 'bracketPair': return { d: `M${r2(ss * 0.15)} 0Q0 0 0 ${r2(ss * 0.15)}V${r2(h - ss * 0.15)}Q0 ${r2(h)} ${r2(ss * 0.15)} ${r2(h)}M${r2(w - ss * 0.15)} 0Q${r2(w)} 0 ${r2(w)} ${r2(ss * 0.15)}V${r2(h - ss * 0.15)}Q${r2(w)} ${r2(h)} ${r2(w - ss * 0.15)} ${r2(h)}`, open: true };
    case 'lightningBolt': return { d: polygon([[w * 0.4, 0], [w * 0.75, h * 0.4], [w * 0.55, h * 0.45], [w, h], [w * 0.35, h * 0.6], [w * 0.5, h * 0.55], [0, h * 0.3]]) };
    case 'flowChartOffpageConnector': return { d: polygon([[0, 0], [w, 0], [w, h * 0.8], [w / 2, h], [0, h * 0.8]]) };
    case 'flowChartDelay': return { d: `M0 0H${r2(w / 2)}A${r2(w / 2)} ${r2(h / 2)} 0 0 1 ${r2(w / 2)} ${r2(h)}H0Z` };
    case 'flowChartDisplay': return { d: polygon([[0, h / 2], [w * 0.15, 0], [w * 0.85, 0], [w, h / 2], [w * 0.85, h], [w * 0.15, h]]) };
    case 'flowChartMagneticDisk': case 'flowChartMagneticDrum': case 'flowChartMagneticTape': return { d: roundRectPath(w, h, ss * 0.2) };
    case 'corner': { const a = ss * A(adj, 'adj1', 50000), b = ss * A(adj, 'adj2', 50000); return { d: polygon([[0, 0], [b, 0], [b, h - a], [w, h - a], [w, h], [0, h]]) }; }
    case 'quadArrow': case 'leftRightUpArrow': case 'uturnArrow': case 'bentArrow': case 'bentUpArrow': case 'curvedRightArrow': case 'curvedLeftArrow': case 'curvedUpArrow': case 'curvedDownArrow': case 'circularArrow': case 'swooshArrow':
      return { d: arrowPts(w, h, adj, 'right') };
    case 'leftArrowCallout': case 'rightArrowCallout': case 'upArrowCallout': case 'downArrowCallout': case 'borderCallout1': case 'borderCallout2': case 'borderCallout3': case 'accentCallout1': case 'accentCallout2': case 'accentCallout3': case 'callout1': case 'callout2': case 'callout3': case 'accentBorderCallout1': case 'accentBorderCallout2': case 'accentBorderCallout3':
      return { d: `M0 0H${r2(w)}V${r2(h)}H0Z` };
    case 'ribbon': case 'ribbon2': case 'ellipseRibbon': case 'ellipseRibbon2': case 'verticalScroll': case 'horizontalScroll': case 'wave': case 'doubleWave': case 'foldedCorner': case 'halfFrame': case 'diagStripe': case 'chord': case 'funnel': case 'gear6': case 'gear9': case 'pieWedge': case 'mathMinus': case 'mathMultiply': case 'mathDivide': case 'mathEqual': case 'mathNotEqual': case 'leftRightRibbon': case 'leftUpArrow': case 'leftCircularArrow': case 'leftRightCircularArrow': case 'chartPlus': case 'chartStar': case 'chartX': case 'cornerTabs': case 'squareTabs': case 'plaqueTabs': case 'inverseRibbon': case 'trapezoid2': case 'star3': case 'actionButtonHome': case 'actionButtonHelp': case 'actionButtonInformation': case 'actionButtonForwardNext': case 'actionButtonBackPrevious': case 'actionButtonEnd': case 'actionButtonBeginning': case 'actionButtonReturn': case 'actionButtonDocument': case 'actionButtonSound': case 'actionButtonMovie': case 'flowChartPunchedCard': case 'flowChartInputOutput': case 'flowChartCollate': case 'flowChartSort': case 'flowChartStoredData': case 'flowChartSequentialAccessStorage': case 'flowChartOnlineStorage':
      return { d: `M0 0H${r2(w)}V${r2(h)}H0Z` };
    default: return null;
  }
}
/** 사용자 지정 모양 → 경로 목록 (shape 크기에 맞춰 늘림) */
function customPaths(custom, w, h) {
  return (custom || []).map(p => {
    const sx = p.w ? w / p.w : 1, sy = p.h ? h / p.h : 1;
    let d = '';
    for (const c of p.cmds) {
      if (c[0] === 'Z') { d += 'Z'; continue; }
      if (c[0] === 'A') { d += `A${r2(c[1] * sx)} ${r2(c[2] * sy)} 0 ${c[4]} ${c[5]} ${r2(c[6] * sx)} ${r2(c[7] * sy)}`; continue; }
      d += c[0];
      for (let k = 1; k < c.length; k += 2) d += `${r2(c[k] * sx)} ${r2(c[k + 1] * sy)}${k + 2 < c.length ? ' ' : ''}`;
    }
    return { d, fill: p.fill, stroke: p.stroke };
  });
}
/** 도형의 경로들 → [{ d, fill(채움 여부), stroke }] (모르는 모양은 사각형) */
function geomPaths(sh) {
  const g = sh.geom || { prst: 'rect', adj: {} };
  if (g.prst === 'custom') { const ps = customPaths(g.custom, sh.w, sh.h); if (ps.length) return ps; }
  const p = presetPath(g.prst, sh.w, sh.h, g.adj) || presetPath('rect', sh.w, sh.h, {});
  return [{ d: p.d, fill: !p.open, stroke: true }];
}

// ---------------- 채우기 · 선 → SVG 속성 ----------------
class Defs {
  constructor(embed) { this.list = []; this.n = 0; this.embed = embed || (() => null); }
  id(p) { return `${p}${++this.n}`; }
  /** 채우기 → { attrs: 'fill="…" fill-opacity="…"', extra: 그 위에 겹쳐 그릴 SVG (그림) } */
  fill(f, box, clipD) {
    if (!f || f.type === 'none') return { attrs: 'fill="none"', extra: '' };
    if (f.type === 'solid') return { attrs: `fill="${esc(f.color)}"${f.alpha < 1 ? ` fill-opacity="${r2(f.alpha)}"` : ''}`, extra: '' };
    if (f.type === 'grad') {
      const id = this.id('g');
      const stops = f.stops.map(s => `<stop offset="${r2(s.pos * 100)}%" stop-color="${esc(s.color)}"${s.alpha < 1 ? ` stop-opacity="${r2(s.alpha)}"` : ''}/>`).join('');
      if (f.path) this.list.push(`<radialGradient id="${id}" cx="50%" cy="50%" r="70%">${stops}</radialGradient>`);
      else {
        const a = (f.angle || 0) * Math.PI / 180;
        const x1 = 50 - 50 * Math.cos(a), y1 = 50 - 50 * Math.sin(a), x2 = 50 + 50 * Math.cos(a), y2 = 50 + 50 * Math.sin(a);
        this.list.push(`<linearGradient id="${id}" x1="${r2(x1)}%" y1="${r2(y1)}%" x2="${r2(x2)}%" y2="${r2(y2)}%">${stops}</linearGradient>`);
      }
      return { attrs: `fill="url(#${id})"`, extra: '' };
    }
    if (f.type === 'pattern') return { attrs: `fill="${esc(f.fg)}" fill-opacity="0.5"`, extra: '' };
    if (f.type === 'blip') {
      const uri = f.part ? this.embed(f.part) : null;
      if (!uri) return { attrs: 'fill="none"', extra: '' };
      const img = imageTag(uri, box, f.crop, f.alpha);
      if (!clipD) return { attrs: 'fill="none"', extra: img };
      const id = this.id('c');
      this.list.push(`<clipPath id="${id}"><path d="${clipD}"/></clipPath>`);
      return { attrs: 'fill="none"', extra: `<g clip-path="url(#${id})">${img}</g>` };
    }
    return { attrs: 'fill="none"', extra: '' };
  }
  stroke(ln) {
    if (!ln) return 'stroke="none"';
    const w = Math.max(0.25, ln.w);
    const dash = ln.dash ? ({ dash: `${w * 4} ${w * 3}`, sysDash: `${w * 3} ${w}`, dot: `${w} ${w * 3}`, sysDot: `${w} ${w}`, lgDash: `${w * 8} ${w * 3}`, dashDot: `${w * 4} ${w * 3} ${w} ${w * 3}`, lgDashDot: `${w * 8} ${w * 3} ${w} ${w * 3}`, sysDashDot: `${w * 3} ${w} ${w} ${w}`, lgDashDotDot: `${w * 8} ${w * 3} ${w} ${w * 3} ${w} ${w * 3}` })[ln.dash] : '';
    return `stroke="${esc(ln.color)}" stroke-width="${r2(w)}"${ln.alpha < 1 ? ` stroke-opacity="${r2(ln.alpha)}"` : ''}${dash ? ` stroke-dasharray="${dash}"` : ''}`;
  }
  /** 선 끝 화살표 */
  marker(ln, color) {
    const id = this.id('m');
    this.list.push(`<marker id="${id}" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="4" markerHeight="4" orient="auto-start-reverse"><path d="M0 0L10 5L0 10Z" fill="${esc(color)}"/></marker>`);
    return id;
  }
}
/** 그림을 상자(box) 에 꽉 채워 (자르기 srcRect 는 그림을 키우고 상자 밖은 잘라서) */
function imageTag(uri, box, crop, alpha = 1) {
  const { x, y, w, h } = box;
  let ix = x, iy = y, iw = w, ih = h;
  if (crop) { // 자른 그림: 그림을 키워 놓고 상자 밖은 안쪽 <svg> 가 잘라 낸다
    const cw = Math.max(0.01, 1 - crop.l - crop.r), ch = Math.max(0.01, 1 - crop.t - crop.b);
    iw = w / cw; ih = h / ch; ix = x - iw * crop.l; iy = y - ih * crop.t;
  }
  const img = `<image href="${uri}" x="${r2(ix)}" y="${r2(iy)}" width="${r2(iw)}" height="${r2(ih)}" preserveAspectRatio="none"${alpha < 1 ? ` opacity="${r2(alpha)}"` : ''}/>`;
  if (!crop) return img;
  return `<svg x="${r2(x)}" y="${r2(y)}" width="${r2(w)}" height="${r2(h)}" viewBox="${r2(x)} ${r2(y)} ${r2(w)} ${r2(h)}" overflow="hidden">${img}</svg>`;
}

// ---------------- 글 → HTML (foreignObject) ----------------
const ALIGN = { l: 'left', ctr: 'center', r: 'right', just: 'justify', justLow: 'justify', dist: 'justify', thaiDist: 'justify' };
const ANCHOR = { t: 'flex-start', ctr: 'center', b: 'flex-end', just: 'flex-start', dist: 'flex-start' };
function runHtml(r) {
  const fam = r.ea && HANGUL.test(r.text) ? [r.ea, r.latin] : [r.latin, r.ea];
  const family = fam.filter(Boolean).map(f => `'${cssFont(f)}'`).concat(['sans-serif']).join(',');
  let text = r.text;
  if (r.cap === 'all') text = text.toUpperCase();
  const st = `font-family:${esc(family)};font-size:${r2(r.sz || 18)}px;color:${esc(r.color || '#000')};font-weight:${r.b ? 700 : 400};${r.i ? 'font-style:italic;' : ''}${r.u ? 'text-decoration:underline;' : ''}${r.strike ? 'text-decoration:line-through;' : ''}${r.cap === 'small' ? 'font-variant:small-caps;' : ''}${r.baseline ? `vertical-align:${r.baseline > 0 ? 'super' : 'sub'};font-size:${r2((r.sz || 18) * 0.65)}px;` : ''}`;
  return `<span style="${st}">${esc(text).replace(/\n/g, '<br/>')}</span>`;
}
/** 글 몸통 → <div> HTML (xhtml, foreignObject 안에 넣는다) · box = { w, h } 포인트 */
function textHtml(text, box, { pad = true } = {}) {
  if (!text || !text.paras.length) return '';
  const bp = text.bodyPr;
  const vert = bp.vert && bp.vert !== 'horz' ? 'writing-mode:vertical-rl;' : '';
  const paras = text.paras.map(p => {
    const runs = p.runs.filter(r => r.text !== '');
    const sz = runs.find(r => r.sz) ? Math.max(...runs.filter(r => r.sz).map(r => r.sz)) : p.endSz || 18;
    const lh = p.lnSpc ? (p.lnSpc.pct ? `${r2(p.lnSpc.pct * 1.2)}` : `${r2(p.lnSpc.pts)}px`) : '1.2';
    const spc = s => (s ? (s.pts != null ? s.pts : s.pct * sz * 1.2) : 0);
    const marL = p.marL || 0, indent = p.indent || 0;
    const bullet = p.bullet ? `<span style="display:inline-block;min-width:${r2(Math.max(0, -indent))}px;margin-left:${r2(indent)}px;padding-right:0.3em">${esc(p.bullet)}</span>` : '';
    const body = runs.length ? runs.map(r => runHtml(r.br ? { ...(runs.find(x => !x.br) || {}), text: '\n' } : r)).join('') : `<span style="font-size:${r2(sz)}px">&#160;</span>`;
    return `<p style="margin:${r2(spc(p.spcBef))}px 0 ${r2(spc(p.spcAft))}px 0;padding-left:${r2(marL)}px;${p.bullet ? '' : `text-indent:${r2(indent)}px;`}text-align:${ALIGN[p.algn] || 'left'};line-height:${lh};font-size:${r2(sz)}px;white-space:pre-wrap;word-break:break-word">${bullet}${body}</p>`;
  }).join('');
  const padding = pad ? `${r2(bp.tIns)}px ${r2(bp.rIns)}px ${r2(bp.bIns)}px ${r2(bp.lIns)}px` : '0';
  return `<div xmlns="http://www.w3.org/1999/xhtml" style="display:flex;flex-direction:column;justify-content:${ANCHOR[bp.anchor] || 'flex-start'};box-sizing:border-box;width:${r2(box.w)}px;height:${r2(box.h)}px;padding:${padding};overflow:visible;${vert}${bp.wrap ? '' : 'white-space:nowrap;'}">${paras}</div>`;
}
/** 표 → HTML <table> */
function tableHtml(t, box) {
  const cols = t.cols.length ? t.cols : [box.w];
  const accent = t.accent || '#4472c4';
  const light = mix(accent, '#ffffff', 0.8), lighter = mix(accent, '#ffffff', 0.9);
  const rows = t.rows.map((r, ri) => {
    const cells = r.cells.map((c, ci) => {
      if (c.merged) return '';
      let bg = c.fill && c.fill.type === 'solid' ? c.fill.color : c.fill && c.fill.type === 'grad' ? c.fill.stops[0].color : '';
      let bold = false, color = '';
      if (!bg && t.styled) { if (t.firstRow && ri === 0) { bg = accent; bold = true; color = '#ffffff'; } else bg = t.bandRow && ri % 2 === 0 ? light : lighter; }
      const txt = c.text ? { ...c.text, paras: c.text.paras.map(p => ({ ...p, runs: p.runs.map(x => ({ ...x, b: bold || x.b, color: color || x.color })) })) } : null;
      const b = side => { const v = c.borders && c.borders[side]; return v === null ? 'none' : v ? `${r2(v.w)}px solid ${v.color}` : t.styled ? '1px solid #ffffff' : '1px solid #000'; };
      const w = cols.slice(ci, ci + c.span).reduce((a, b2) => a + b2, 0);
      const inner = txt ? textHtml({ ...txt, bodyPr: { ...txt.bodyPr, anchor: c.anchor, lIns: c.ins.l, rIns: c.ins.r, tIns: c.ins.t, bIns: c.ins.b, wrap: true } }, { w, h: r.h }).replace(/^<div xmlns="[^"]+"/, '<div') : '';
      return `<td colspan="${c.span}" rowspan="${c.rowSpan}" style="padding:0;vertical-align:top;width:${r2(w)}px;height:${r2(r.h)}px;background:${bg || 'transparent'};border-left:${b('L')};border-right:${b('R')};border-top:${b('T')};border-bottom:${b('B')}">${inner}</td>`;
    }).join('');
    return `<tr style="height:${r2(r.h)}px">${cells}</tr>`;
  }).join('');
  return `<div xmlns="http://www.w3.org/1999/xhtml" style="width:${r2(box.w)}px;height:${r2(box.h)}px;overflow:visible"><table style="border-collapse:collapse;table-layout:fixed;width:${r2(cols.reduce((a, b) => a + b, 0))}px">${rows}</table></div>`;
}
function mix(a, b, t) {
  const p = s => [1, 3, 5].map(i => parseInt(s.slice(i, i + 2), 16));
  const [r1, g1, b1] = p(a), [r2_, g2, b2] = p(b);
  const h = v => Math.round(v).toString(16).padStart(2, '0');
  return '#' + h(r1 + (r2_ - r1) * t) + h(g1 + (g2 - g1) * t) + h(b1 + (b2 - b1) * t);
}

// ---------------- 도형 하나 → SVG ----------------
function transformOf(sh) {
  const cx = sh.x + sh.w / 2, cy = sh.y + sh.h / 2;
  const parts = [];
  for (const g of sh.gt || []) {
    if (g.rot) parts.push(`rotate(${r2(g.rot)} ${r2(g.cx)} ${r2(g.cy)})`);
    if (g.flipH || g.flipV) parts.push(`translate(${r2(g.cx)} ${r2(g.cy)}) scale(${g.flipH ? -1 : 1} ${g.flipV ? -1 : 1}) translate(${r2(-g.cx)} ${r2(-g.cy)})`);
  }
  if (sh.rot) parts.push(`rotate(${r2(sh.rot)} ${r2(cx)} ${r2(cy)})`);
  if (sh.flipH || sh.flipV) parts.push(`translate(${r2(cx)} ${r2(cy)}) scale(${sh.flipH ? -1 : 1} ${sh.flipV ? -1 : 1}) translate(${r2(-cx)} ${r2(-cy)})`);
  return parts.join(' ');
}
/**
 * 도형 → SVG 조각. opts.noText = 글은 빼고(글상자로 따로 꺼낼 때) · opts.noBody = 몸통은 빼고 글만
 * 좌표는 슬라이드 기준 그대로 (translate 로 옮긴다)
 */
function shapeSvg(sh, defs, opts = {}) {
  if (!sh || !(sh.w >= 0) || !(sh.h >= 0)) return '';
  if (sh.dgmShapes) return sh.dgmShapes.map(d => shapeSvg(d, defs, opts)).join('');
  const tr = transformOf(sh);
  const inner = [];
  const box = { x: 0, y: 0, w: sh.w, h: sh.h };
  if (!opts.noBody) {
    if (sh.kind === 'table' && sh.table) {
      inner.push(`<foreignObject x="0" y="0" width="${r2(sh.w)}" height="${r2(sh.h)}" overflow="visible">${tableHtml(sh.table, box)}</foreignObject>`);
    } else if (sh.kind === 'chart' || sh.kind === 'graphic') {
      // 차트는 그릴 자료가 없다 (PowerPoint 가 그린다) — 비워 둔다
    } else {
      const paths = geomPaths(sh);
      const clipD = paths.length === 1 && paths[0].fill ? paths[0].d : '';
      const stroke = defs.stroke(sh.line);
      // 그림 (사진 · 동영상 포스터 · OLE 미리 보기): 모양대로 잘라서
      if (sh.pic && sh.pic.type === 'blip') {
        const f = defs.fill(sh.pic, box, sh.geom && sh.geom.prst !== 'rect' ? clipD : '');
        inner.push(f.extra);
        if (sh.line) inner.push(`<path d="${clipD || paths[0].d}" fill="none" ${stroke}/>`);
      } else {
        const f = defs.fill(sh.fill, box, clipD);
        let markers = '';
        if (sh.line && sh.kind === 'cxnSp') {
          if (sh.line.tail && sh.line.tail !== 'none') markers += ` marker-end="url(#${defs.marker(sh.line, sh.line.color)})"`;
          if (sh.line.head && sh.line.head !== 'none') markers += ` marker-start="url(#${defs.marker(sh.line, sh.line.color)})"`;
        }
        for (const p of paths) inner.push(`<path d="${p.d}" ${p.fill ? f.attrs : 'fill="none"'} ${p.stroke ? stroke : 'stroke="none"'}${markers}${sh.line && sh.line.w > 2 ? ' stroke-linejoin="round"' : ''}/>`);
        if (f.extra) inner.push(f.extra);
      }
    }
  }
  if (!opts.noText && sh.text && sh.text.hasText) inner.push(`<foreignObject x="0" y="0" width="${r2(sh.w)}" height="${r2(sh.h)}" overflow="visible">${textHtml(sh.text, box)}</foreignObject>`);
  if (!inner.join('')) return '';
  return `<g transform="${tr ? tr + ' ' : ''}translate(${r2(sh.x)} ${r2(sh.y)})">${inner.join('')}</g>`;
}

/** 배경 채우기 → SVG (슬라이드 가득) */
function bgSvg(bg, w, h, defs) {
  if (!bg || bg.type === 'none') return '';
  const f = defs.fill(bg, { x: 0, y: 0, w, h }, '');
  return `<rect x="0" y="0" width="${r2(w)}" height="${r2(h)}" ${f.attrs}/>${f.extra}`;
}

/**
 * 슬라이드 → SVG 글 (slide = parseSlide 결과)
 *   opts.skip = 빼고 그릴 도형(Set) · opts.textOnly = 글만 빼고 몸통은 그릴 도형(Set) · opts.embed(part) → data: 주소 · opts.width = 그림 가로 px
 */
function slideSvg(slide, opts = {}) {
  const W = slide.w, H = slide.h;
  const defs = new Defs(opts.embed);
  const skip = opts.skip || new Set(), textOnly = opts.textOnly || new Set();
  const body = [bgSvg(slide.bg, W, H, defs)];
  for (const sh of slide.shapes) {
    if (skip.has(sh)) continue;
    body.push(shapeSvg(sh, defs, { noText: textOnly.has(sh) }));
  }
  const pxW = opts.width || 1920, pxH = Math.round(pxW * H / W);
  return `<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="${pxW}" height="${pxH}" viewBox="0 0 ${r2(W)} ${r2(H)}">${defs.list.length ? `<defs>${defs.list.join('')}</defs>` : ''}${body.join('')}</svg>`;
}

/**
 * 도형 하나짜리 SVG (요소 그림으로 꺼낼 때) → { svg, l, t, w, h } — 테두리 선 두께만큼 상자를 넓힌다
 */
function elementSvg(sh, opts = {}) {
  const defs = new Defs(opts.embed);
  const pad = sh.line ? Math.max(1, sh.line.w) : 0.5;
  const l = sh.x - pad, t = sh.y - pad, w = sh.w + pad * 2, h = sh.h + pad * 2;
  const inner = shapeSvg(sh, defs, { noText: true });
  if (!inner) return null;
  const pxW = Math.max(8, Math.round(w * (opts.scale || 2))), pxH = Math.max(8, Math.round(h * (opts.scale || 2)));
  return { svg: `<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="${pxW}" height="${pxH}" viewBox="${r2(l)} ${r2(t)} ${r2(w)} ${r2(h)}">${defs.list.length ? `<defs>${defs.list.join('')}</defs>` : ''}${inner}</svg>`, l, t, w, h };
}

module.exports = { slideSvg, elementSvg, shapeSvg, textHtml, presetPath, geomPaths, imageTag, Defs };
