'use strict';
// 실제 PowerPoint 로 확인하기 — PowerPoint 가 깔린 윈도우 PC 에서 (1.14.0 의 PowerPoint 연동 두 가지)
//   ① 클립보드의 PowerPoint 슬라이드 (src/core/clippaste.js)   ② 요소 따로 고치기의 배경 그림 (src/core/pptrender.js PS_BG)
//
//   node tools/ppt-check.js          ①의 1~4 와 ②를 차례로 돌리고 결과를 적는다 (2~3분, PowerPoint 창이 잠깐 뜬다)
//   node tools/ppt-check.js clip     지금 클립보드로 ①의 1~4 를 다시 (PowerPoint 에서 손으로 복사한 뒤)
//   node tools/ppt-check.js app      임시 데이터 폴더 · 다른 포트로 프로그램을 띄운다 (쓰던 프로그램과 따로)
//   node tools/ppt-check.js log      app 으로 띄운 프로그램에서 해 본 것(붙여 넣기 · 요소 따로 고치기)의 기록을 모은다
//
// 결과: <임시 폴더>/wc-ppt-check/out/result.txt (글) · compare.html (그림 견주기)
// 클립보드 내용이 바뀐다. 사용자가 열어 둔 프레젠테이션은 건드리지 않는다 (확인용으로 연 것만 닫는다).
// 코드의 스크립트(PS_CLIP_DUMP · PS_CLIP_PASTE · PS_BG)는 고치지 않고 그대로 돌린다.

const fs = require('fs');
const os = require('os');
const path = require('path');
const zlib = require('zlib');
const crypto = require('crypto');
const { execFile, spawn } = require('child_process');

const ROOT = [path.join(__dirname, '..'), process.cwd(), __dirname].find(d => fs.existsSync(path.join(d, 'src', 'core', 'clippaste.js'))) || path.join(__dirname, '..');
const BASE = path.join(os.tmpdir(), 'wc-ppt-check');
const OUT = path.join(BASE, 'out');
const DATA = path.join(BASE, 'data');
const PORT = 37620;
const core = name => require(path.join(ROOT, 'src', 'core', name));
const wait = ms => new Promise(r => setTimeout(r, ms));
const sha1 = buf => crypto.createHash('sha1').update(buf).digest('hex').slice(0, 12);

// ---------------- 결과 적기 ----------------
const lines = [];
function say(s = '') { for (const l of String(s).split('\n')) { lines.push(l); console.log(l); } }
function head(s) { say(''); say('=== ' + s + ' ==='); }
const html = []; // compare.html 의 조각
const esc = s => String(s).replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');
function saveResults(name = 'result.txt') {
  fs.mkdirSync(OUT, { recursive: true });
  fs.writeFileSync(path.join(OUT, name), '﻿' + lines.join('\r\n') + '\r\n');
  if (html.length) {
    fs.writeFileSync(path.join(OUT, name.replace(/\.txt$/, '.html').replace('result', 'compare')), '<!doctype html><meta charset="utf-8"><title>PowerPoint 확인</title>'
      + '<style>body{font:14px sans-serif;background:#222;color:#eee;margin:16px}h2{margin:28px 0 8px}.row{display:flex;gap:10px;flex-wrap:wrap;margin:6px 0 18px}'
      + 'figure{margin:0}figcaption{font-size:12px;color:#bbb;margin-bottom:3px}.im{position:relative;width:440px;border:1px solid #555;line-height:0;background:#fff}.im img{width:100%}'
      + '.bx{position:absolute;border:1.5px solid #f0f;box-sizing:border-box;font:10px sans-serif;color:#f0f;line-height:1;overflow:hidden}</style>' + html.join('\n'));
  }
}
/** 그림 한 칸 (boxes: [{l,t,w,h,label}] 은 슬라이드에 대한 % ) */
function fig(caption, file, boxes = []) {
  const src = path.isAbsolute(file) ? 'file:///' + file.replace(/\\/g, '/') : file;
  return `<figure><figcaption>${esc(caption)}</figcaption><div class="im"><img src="${esc(src)}">`
    + boxes.map(b => `<div class="bx" style="left:${b.l}%;top:${b.t}%;width:${b.w}%;height:${b.h}%">${esc(b.label || '')}</div>`).join('') + '</div></figure>';
}

// ---------------- PowerShell ----------------
function decode(buf) {
  if (!buf || !buf.length) return '';
  let s = buf.toString('utf8');
  if (s.includes('�')) { try { s = new TextDecoder('euc-kr').decode(buf); } catch { /* 그대로 */ } }
  s = s.replace(/^﻿/, '');
  if (s.startsWith('#< CLIXML')) { // -EncodedCommand 로 돌리면 오류가 XML 로 온다
    const parts = [...s.matchAll(/<S S="Error">([\s\S]*?)<\/S>/g)].map(m => m[1].replace(/_x000D__x000A_/g, '\n').replace(/&lt;/g, '<').replace(/&gt;/g, '>').replace(/&amp;/g, '&'));
    s = parts.join('').trim();
  }
  return s;
}
/** 스크립트 실행 → { ok, out, err } (코드의 runPs 와 같은 인수. sta: 클립보드는 STA 에서만 읽힌다) */
function ps(script, env = {}, { timeout = 90000, sta = true } = {}) {
  const enc = Buffer.from(script, 'utf16le').toString('base64');
  const args = ['-NoProfile', '-NonInteractive', ...(sta ? ['-STA'] : []), '-ExecutionPolicy', 'Bypass', '-EncodedCommand', enc];
  return new Promise(resolve => {
    execFile('powershell.exe', args, { windowsHide: true, timeout, encoding: 'buffer', maxBuffer: 32 * 1024 * 1024, env: { ...process.env, ...env } }, (err, stdout, stderr) => {
      const e = decode(stderr);
      resolve({ ok: !err, out: decode(stdout), err: (err && err.killed ? `(시간 ${Math.round(timeout / 1000)}초를 넘김) ` : '') + (e || (err ? String(err.message).split('\n')[0] : '')) });
    });
  });
}
const firstLines = (s, n = 3) => String(s || '').split('\n').map(l => l.trim()).filter(Boolean).slice(0, n).join(' / ');

const PS_HEAD = `
$ErrorActionPreference = 'Stop'
try { [Console]::OutputEncoding = [System.Text.Encoding]::UTF8 } catch { }
`;
const PS_TAIL = `
finally {
  if ($p) { try { $p.Saved = -1; $p.Close() } catch { } }
  try { if ($pp.Presentations.Count -eq 0) { $pp.Quit() } } catch { }
}
`;

// 원본을 열어 슬라이드 1·2 를 복사하고, 끝내라는 표시(WC_DONE 파일)가 생길 때까지 열어 둔다 (사람이 PowerPoint 에서 복사해 둔 상태와 같게: 창 있음)
//   WC_43 = 1 이면 4:3 으로 바꾼 뒤에 (WC_SRC 는 임시 폴더의 사본이라 원본 파일은 그대로)
const PS_HOLD = PS_HEAD + `
$pp = New-Object -ComObject PowerPoint.Application
$p = $null
try {
  $before = $pp.Presentations.Count
  try { $pp.Visible = -1 } catch { }
  if ($env:WC_43 -eq '1') {
    $p = $pp.Presentations.Open($env:WC_SRC, 0, 0, -1)
    $p.PageSetup.SlideSize = 1
  } else {
    $p = $pp.Presentations.Open($env:WC_SRC, -1, 0, -1)
  }
  try { $p.Slides.Range(@(1, 2)).Copy() } catch { $p.Slides.Range([int[]](1, 2)).Copy() }
  $w = $p.PageSetup.SlideWidth; $h = $p.PageSetup.SlideHeight
  $info = @("COPIED slides=$($p.Slides.Count) size=$($w)x$($h) openBefore=$before version=$($pp.Version) build=$($pp.Build)")
  foreach ($k in 1, 2) {
    $s = $p.Slides.Item($k)
    $s.Export((Join-Path $env:WC_OUT ($env:WC_TAG + '-src-' + $k + '.png')), 'PNG', 960, [int](960 * $h / $w))
    $info += "SRC $k design=$($s.Design.Name) layout=$($s.CustomLayout.Name)"
  }
  Set-Content -LiteralPath $env:WC_READY -Value $info -Encoding UTF8
  $t = 0
  while ((-not (Test-Path -LiteralPath $env:WC_DONE)) -and ($t -lt 3000)) { Start-Sleep -Milliseconds 200; $t++ }
} catch {
  Set-Content -LiteralPath $env:WC_READY -Value ('FAIL ' + $_.Exception.Message) -Encoding UTF8
}` + PS_TAIL;

// 클립보드의 형식 이름과 내용 모양
const PS_FORMATS = PS_HEAD + `
Add-Type -AssemblyName System.Windows.Forms
$d = [System.Windows.Forms.Clipboard]::GetDataObject()
if (-not $d) { Write-Output 'NODATA'; exit }
$names = @($d.GetFormats($false))
foreach ($f in $names) { Write-Output ('NAME' + [char]9 + $f) }
foreach ($f in $names) {
  $what = ''
  try {
    $v = $d.GetData($f, $false)
    if ($null -eq $v) { $what = 'null' }
    elseif ($v -is [System.IO.MemoryStream]) { $b = $v.ToArray(); $what = 'MemoryStream ' + $b.Length + ' ' + [System.BitConverter]::ToString($b, 0, [Math]::Min(8, $b.Length)) }
    elseif ($v -is [byte[]]) { $what = 'bytes ' + $v.Length + ' ' + [System.BitConverter]::ToString($v, 0, [Math]::Min(8, $v.Length)) }
    elseif ($v -is [string]) { $what = 'String ' + $v.Length }
    else { $what = $v.GetType().FullName }
  } catch { $what = 'ERR ' + ($_.Exception.Message -replace '\\s+', ' ') }
  Write-Output ('DATA' + [char]9 + $f + [char]9 + $what)
}
`;

// 파일을 PowerPoint 로 열어 본다 (읽기 전용 · 창 없이) + 슬라이드마다 PNG
const PS_OPEN = PS_HEAD + `
$pp = New-Object -ComObject PowerPoint.Application
$p = $null
try {
  $p = $pp.Presentations.Open($env:WC_FILE, -1, 0, 0)
  $w = $p.PageSetup.SlideWidth; $h = $p.PageSetup.SlideHeight
  Write-Output "OPEN slides=$($p.Slides.Count) size=$($w)x$($h)"
  $k = 0
  foreach ($s in $p.Slides) {
    $k++
    $s.Export((Join-Path $env:WC_OUT ($env:WC_TAG + '-' + $k + '.png')), 'PNG', 960, [int](960 * $h / $w))
    Write-Output "SLIDE $k design=$($s.Design.Name) layout=$($s.CustomLayout.Name) shapes=$($s.Shapes.Count)"
  }
} catch {
  Write-Output ('FAIL ' + $_.Exception.Message)
}` + PS_TAIL;

// 도형마다 COM 의 Shape.Id · 이름 · 숨겨지는가 (그룹 안까지. 읽기 전용으로 열고 저장하지 않는다)
const PS_SHAPES = PS_HEAD + `
$pp = New-Object -ComObject PowerPoint.Application
$p = $null
function Walk($shapes, $slide, $depth) {
  foreach ($x in $shapes) {
    if ($x.Type -eq 6) { Walk $x.GroupItems $slide ($depth + 1) }
    $hide = 'ok'
    try { $x.Visible = 0; if ($x.Visible -ne 0) { $hide = 'stays' } } catch { $hide = 'ERR ' + ($_.Exception.Message -replace '\\s+', ' ') }
    Write-Output (('SH', $slide, $depth, $x.Id, $x.Type, $hide, $x.Name) -join [char]9)
  }
}
try {
  $p = $pp.Presentations.Open($env:WC_FILE, -1, 0, 0)
  foreach ($s in $p.Slides) {
    Write-Output (('SL', $s.SlideIndex, $s.SlideShowTransition.Hidden) -join [char]9)
    Walk $s.Shapes $s.SlideIndex 0
  }
} catch {
  Write-Output ('FAIL ' + $_.Exception.Message)
}` + PS_TAIL;

// ---------------- .pptx 들여다보기 ----------------
const attr = (tag, name) => { const m = new RegExp('\\b' + name + '="([^"]*)"').exec(tag || ''); return m ? m[1] : ''; };
const resolvePart = (from, target) => path.posix.normalize(target.startsWith('/') ? target.slice(1) : path.posix.join(path.posix.dirname(from), target));
function rels(z, part) {
  const b = z.get(path.posix.join(path.posix.dirname(part), '_rels', path.posix.basename(part) + '.rels'));
  if (!b) return [];
  return [...b.toString('utf8').matchAll(/<Relationship\b[^>]*>/g)].map(m => ({ id: attr(m[0], 'Id'), type: attr(m[0], 'Type').split('/').pop(), target: resolvePart(part, attr(m[0], 'Target')) }));
}
/** 슬라이드 XML 의 도형 { id, name } (슬라이드 뿌리 묶음 id=1 은 뺀다) */
function xmlShapes(xml) {
  return [...String(xml).matchAll(/<p:cNvPr\b[^>]*>/g)].slice(1).map(m => ({ id: attr(m[0], 'id'), name: attr(m[0], 'name') }));
}
function themeInfo(z, part) {
  const x = (z.get(part) || Buffer.alloc(0)).toString('utf8');
  const font = k => attr((new RegExp('<a:' + k + 'Font>\\s*(<a:latin\\b[^>]*>)').exec(x) || [])[1], 'typeface');
  const ea = k => attr((new RegExp('<a:' + k + 'Font>[\\s\\S]*?(<a:ea\\b[^>]*>)').exec(x) || [])[1], 'typeface');
  const accent = (/<a:accent1>\s*<a:srgbClr\b[^>]*val="([0-9A-Fa-f]{6})"/.exec(x) || [])[1] || '';
  return `${attr((/<a:theme\b[^>]*>/.exec(x) || [])[0], 'name')} · 색 ${attr((/<a:clrScheme\b[^>]*>/.exec(x) || [])[0], 'name')}(강조1 ${accent}) · 글꼴 ${font('major')}/${font('minor')} 한글 ${ea('major') || '-'}/${ea('minor') || '-'}`;
}
/** .pptx 요약 → { ok, why, size, slides: [{ part, shapes, layout, theme }], masters, layouts, themes } */
function pptxInfo(buf) {
  const R = { ok: false, why: '', bytes: buf.length, head: buf.subarray(0, 8).toString('hex') };
  if (buf[0] !== 0x50 || buf[1] !== 0x4b) {
    const at = buf.indexOf(Buffer.from([0x50, 0x4b, 3, 4]));
    R.why = `PK 로 시작하지 않음 (처음 8바이트 ${R.head}${R.head.startsWith('d0cf11e0') ? ' = OLE 복합 문서' : ''}${at > 0 ? ` · PK 는 ${at}바이트째에 있음` : ''})`;
    return R;
  }
  let z;
  try { z = core('pptrender').readZip(buf); } catch (e) { R.why = 'zip 으로 읽지 못함: ' + e.message; return R; }
  R.entries = z.names.length;
  const need = ['[Content_Types].xml', 'ppt/presentation.xml'];
  const miss = need.filter(n => !z.names.includes(n));
  if (miss.length) { R.why = 'zip 이지만 없음: ' + miss.join(', ') + ' (들어 있는 것: ' + z.names.slice(0, 12).join(', ') + (z.names.length > 12 ? ' …' : '') + ')'; return R; }
  const pres = z.get('ppt/presentation.xml').toString('utf8');
  const sz = (/<p:sldSz\b[^>]*>/.exec(pres) || [])[0];
  R.size = `${attr(sz, 'cx')}×${attr(sz, 'cy')} EMU (${(attr(sz, 'cx') / attr(sz, 'cy')).toFixed(3)}:1)`;
  R.ratio = attr(sz, 'cx') / attr(sz, 'cy');
  const pr = rels(z, 'ppt/presentation.xml');
  const order = [...pres.matchAll(/<p:sldId\b[^>]*>/g)].map(m => (pr.find(r => r.id === attr(m[0], 'r:id')) || {}).target).filter(Boolean);
  R.slides = order.map(part => {
    const xml = (z.get(part) || Buffer.alloc(0)).toString('utf8');
    const layout = (rels(z, part).find(r => r.type === 'slideLayout') || {}).target || '';
    const master = layout ? (rels(z, layout).find(r => r.type === 'slideMaster') || {}).target || '' : '';
    const theme = master ? (rels(z, master).find(r => r.type === 'theme') || {}).target || '' : '';
    const lname = layout ? attr((/<p:cSld\b[^>]*>/.exec((z.get(layout) || '').toString('utf8')) || [])[0], 'name') : '';
    return { part, shapes: xmlShapes(xml), hidden: /<p:sld\b[^>]*\bshow="0"/.test(xml), layout: `${path.posix.basename(layout)}(${lname})`, master: path.posix.basename(master), theme: theme ? themeInfo(z, theme) : '' };
  });
  R.masters = z.names.filter(n => /^ppt\/slideMasters\/[^/]+\.xml$/.test(n)).length;
  R.layouts = z.names.filter(n => /^ppt\/slideLayouts\/[^/]+\.xml$/.test(n)).length;
  R.ok = R.slides.length > 0;
  if (!R.ok) R.why = '슬라이드가 없음';
  return R;
}
function sayInfo(label, I) {
  if (!I.ok) { say(`  ${label}: .pptx 가 아님 — ${I.why} (${I.bytes}바이트)`); return; }
  say(`  ${label}: ${I.bytes}바이트 · zip ${I.entries}개 · 슬라이드 ${I.slides.length}장 · 크기 ${I.size} · 마스터 ${I.masters} · 레이아웃 ${I.layouts}`);
  I.slides.forEach((s, i) => say(`    ${i + 1}장 ${path.posix.basename(s.part)}${s.hidden ? ' (숨김)' : ''} · ${s.layout} · ${s.master} · 테마 ${s.theme}\n       도형 ${s.shapes.map(x => x.id + ':' + x.name).join(' | ')}`));
}

// ---------------- PNG 견주기 (PowerPoint 가 내보낸 8비트 PNG) ----------------
function readPng(file) {
  const b = fs.readFileSync(file);
  if (b.readUInt32BE(0) !== 0x89504e47) throw new Error('PNG 가 아닙니다');
  let p = 8, w = 0, h = 0, depth = 0, ct = 0, lace = 0, pal = null;
  const idat = [];
  while (p < b.length) {
    const len = b.readUInt32BE(p), type = b.toString('latin1', p + 4, p + 8), d = b.subarray(p + 8, p + 8 + len);
    if (type === 'IHDR') { w = d.readUInt32BE(0); h = d.readUInt32BE(4); depth = d[8]; ct = d[9]; lace = d[12]; }
    else if (type === 'PLTE') pal = d;
    else if (type === 'IDAT') idat.push(d);
    else if (type === 'IEND') break;
    p += 12 + len;
  }
  const ch = { 0: 1, 2: 3, 3: 1, 4: 2, 6: 4 }[ct];
  if (depth !== 8 || lace || !ch) throw new Error(`이 PNG 모양은 못 읽습니다 (depth ${depth} · color ${ct} · interlace ${lace})`);
  const raw = zlib.inflateSync(Buffer.concat(idat));
  const stride = w * ch, cur = Buffer.alloc(stride), prev = Buffer.alloc(stride), rgb = Buffer.alloc(w * h * 3);
  for (let y = 0; y < h; y++) {
    const f = raw[y * (stride + 1)], line = raw.subarray(y * (stride + 1) + 1, (y + 1) * (stride + 1));
    for (let i = 0; i < stride; i++) {
      const a = i >= ch ? cur[i - ch] : 0, up = prev[i], c = i >= ch ? prev[i - ch] : 0;
      let v = line[i];
      if (f === 1) v += a; else if (f === 2) v += up; else if (f === 3) v += (a + up) >> 1;
      else if (f === 4) { const pa = Math.abs(up - c), pb = Math.abs(a - c), pc = Math.abs(a + up - 2 * c); v += pa <= pb && pa <= pc ? a : pb <= pc ? up : c; }
      cur[i] = v & 255;
    }
    for (let x = 0; x < w; x++) {
      const o = (y * w + x) * 3, i = x * ch;
      if (ct === 2 || ct === 6) { rgb[o] = cur[i]; rgb[o + 1] = cur[i + 1]; rgb[o + 2] = cur[i + 2]; }
      else if (ct === 3) { const q = cur[i] * 3; rgb[o] = pal[q]; rgb[o + 1] = pal[q + 1]; rgb[o + 2] = pal[q + 2]; }
      else { rgb[o] = rgb[o + 1] = rgb[o + 2] = cur[i]; }
    }
    cur.copy(prev);
  }
  return { w, h, rgb };
}
/** 두 그림에서 다른 픽셀의 비율(%) — 전체, 그리고 rects(슬라이드에 대한 0~1 {l,t,w,h}) 안·밖. 채널 차이가 24 를 넘으면 다른 픽셀 */
function diffPng(fa, fb, rects = []) {
  const A = readPng(fa), B = readPng(fb);
  if (A.w !== B.w || A.h !== B.h) return { same: false, note: `크기가 다름 ${A.w}×${A.h} / ${B.w}×${B.h}` };
  const inAny = new Uint8Array(A.w * A.h);
  const per = rects.map(r => ({ x0: Math.max(0, Math.floor(r.l * A.w)), y0: Math.max(0, Math.floor(r.t * A.h)), x1: Math.min(A.w, Math.ceil((r.l + r.w) * A.w)), y1: Math.min(A.h, Math.ceil((r.t + r.h) * A.h)), n: 0, d: 0 }));
  for (const r of per) for (let y = r.y0; y < r.y1; y++) for (let x = r.x0; x < r.x1; x++) inAny[y * A.w + x] = 1;
  let all = 0, out = 0, outN = 0;
  const differs = new Uint8Array(A.w * A.h);
  for (let i = 0, o = 0; i < A.w * A.h; i++, o += 3) {
    const d = Math.max(Math.abs(A.rgb[o] - B.rgb[o]), Math.abs(A.rgb[o + 1] - B.rgb[o + 1]), Math.abs(A.rgb[o + 2] - B.rgb[o + 2])) > 24 ? 1 : 0;
    differs[i] = d; all += d;
    if (!inAny[i]) { outN++; out += d; }
  }
  for (const r of per) for (let y = r.y0; y < r.y1; y++) for (let x = r.x0; x < r.x1; x++) { r.n++; r.d += differs[y * A.w + x]; }
  const pct = (d, n) => (n ? Math.round(d / n * 10000) / 100 : 0);
  return { same: true, all: pct(all, A.w * A.h), outside: pct(out, outN), rects: per.map(r => pct(r.d, r.n)) };
}
function sayDiff(label, fa, fb) {
  try {
    if (!fs.existsSync(fa) || !fs.existsSync(fb)) { say(`  ${label}: 그림이 없어 견주지 못함`); return; }
    const d = diffPng(fa, fb);
    say(`  ${label}: ${d.same ? `다른 픽셀 ${d.all}%` : d.note}`);
  } catch (e) { say(`  ${label}: 견주지 못함 — ${e.message}`); }
}

// ---------------- 단계 ----------------
async function preflight() {
  if (process.platform !== 'win32') { console.log('윈도우에서만 됩니다 (PowerPoint 가 깔린 PC 에서 돌려 주세요).'); return false; }
  const ver = await new Promise(res => execFile('reg', ['query', 'HKCR\\PowerPoint.Application\\CurVer'], { windowsHide: true }, (err, out) => res(err ? '' : (String(out).match(/PowerPoint\.Application\.\d+/) || ['있음'])[0])));
  if (!ver) { console.log('이 PC 에는 PowerPoint 가 없습니다 (reg query HKCR\\PowerPoint.Application\\CurVer 실패). 확인할 수 없습니다.'); return false; }
  fs.mkdirSync(OUT, { recursive: true });
  say(`워십프리젠터 PowerPoint 확인 · ${new Date().toLocaleString()} · ${ver} · Node ${process.version} · ${os.release()}`);
  say(`저장소 ${ROOT} · 결과 폴더 ${OUT}`);
  return true;
}

/** 원본(사본)을 PowerPoint 에 열어 슬라이드 1·2 를 복사해 둔다 → { info[], stop() } */
async function hold(tag, fixture, four3) {
  const src = path.join(OUT, `${tag}-source.pptx`);
  fs.copyFileSync(path.join(ROOT, 'tests', 'fixtures', fixture), src);
  const ready = path.join(OUT, `${tag}-ready.txt`), done = path.join(OUT, `${tag}-done.txt`);
  for (const f of [ready, done]) fs.rmSync(f, { force: true });
  let ended = false, res = null;
  const pr = ps(PS_HOLD, { WC_SRC: src, WC_OUT: OUT, WC_TAG: tag, WC_READY: ready, WC_DONE: done, WC_43: four3 ? '1' : '' }, { timeout: 660000, sta: false }).then(r => { ended = true; res = r; });
  for (let i = 0; i < 600 && !fs.existsSync(ready) && !ended; i++) await wait(200); // PowerPoint 가 처음 뜰 때는 느리다 (2분까지)
  await wait(300);
  const info = fs.existsSync(ready) ? fs.readFileSync(ready, 'utf8').replace(/^﻿/, '').split(/\r?\n/).filter(Boolean) : [];
  const stop = async () => { fs.writeFileSync(done, '1'); for (let i = 0; i < 100 && !ended; i++) await wait(200); };
  if (!info.length || /^FAIL/.test(info[0])) {
    say(`  원본을 열어 복사하지 못함: ${info[0] || (res ? firstLines(res.err) : '2분 안에 답이 없음 (PowerPoint 에 창이 떠서 기다리고 있지 않은지 보세요)')}`);
    await stop();
    return null;
  }
  return { info, stop, src };
}

/** ①-1 형식 이름 */
async function stepFormats() {
  head('①-1 클립보드 형식 이름 (GetFormats)');
  const r = await ps(PS_FORMATS, {}, { timeout: 90000 });
  const names = r.out.split(/\r?\n/).filter(l => l.startsWith('NAME\t')).map(l => l.split('\t')[1]);
  const data = r.out.split(/\r?\n/).filter(l => l.startsWith('DATA\t')).map(l => l.split('\t'));
  if (!names.length) { say('  형식을 읽지 못함: ' + (firstLines(r.out) || firstLines(r.err))); return []; }
  for (const n of names) { const d = data.find(x => x[1] === n); say(`  ${n}${d ? '  →  ' + d[2] : ''}`); }
  if (!r.ok) say('  (끝까지 못 읽음: ' + firstLines(r.err) + ')');
  const want = core('clippaste').SLIDE_FORMATS;
  const hit = names.filter(n => /^PowerPoint [0-9.]+ Internal Slides$/.test(n));
  say(`  → 코드가 기대하는 이름 ${JSON.stringify(want)} 가운데 있는 것: ${JSON.stringify(names.filter(n => want.includes(n)))}`);
  say(`  → 'PowerPoint <번호> Internal Slides' 꼴: ${JSON.stringify(hit)} · 그 밖에 PowerPoint 로 시작하는 것: ${JSON.stringify(names.filter(n => /^PowerPoint/i.test(n) && !hit.includes(n)))}`);
  return names;
}

/** ①-2 PS_CLIP_DUMP 그대로 */
async function stepDump(tag) {
  head(`①-2 PS_CLIP_DUMP (코드 그대로) [${tag}]`);
  const cp = core('clippaste');
  const file = path.join(OUT, `${tag}-dump.pptx`);
  fs.rmSync(file, { force: true });
  const r = await ps(cp.PS_CLIP_DUMP, { WC_CLIP_OUT: file }, { timeout: 30000 });
  say(`  출력: ${firstLines(r.out) || '(없음)'}${r.ok ? '' : ' · 오류: ' + firstLines(r.err)}`);
  if (!fs.existsSync(file)) return null;
  const buf = fs.readFileSync(file);
  const I = pptxInfo(buf);
  say(`  looksLikePptx = ${cp.looksLikePptx(buf)} · sha1 ${sha1(buf)}`);
  sayInfo('저장된 파일', I);
  if (I.ok) {
    const o = await ps(PS_OPEN, { WC_FILE: file, WC_OUT: OUT, WC_TAG: `${tag}-dump` }, { sta: false });
    say(`  PowerPoint 로 열기: ${firstLines(o.out, 6) || firstLines(o.err)}`);
  } else say('  (PowerPoint 로 열어 보지 않음 — .pptx 모양이 아님)');
  return { file, sha: sha1(buf), ok: I.ok, info: I };
}

/** ①-3 Electron clipboard.readBuffer */
async function stepElectron(dumpSha) {
  head('①-3 Electron clipboard.has · readBuffer · probeSlides');
  let exe;
  try { exe = require(path.join(ROOT, 'node_modules', 'electron')); } catch { say('  electron 이 없음 — 저장소 폴더에서 npm install 을 먼저 해 주세요.'); return; }
  const script = path.join(OUT, 'electron-clip.js'), outFile = path.join(OUT, 'electron-clip.json');
  fs.rmSync(outFile, { force: true });
  fs.writeFileSync(script, `'use strict';
const { app, clipboard } = require('electron');
const fs = require('fs'); const path = require('path'); const crypto = require('crypto');
const [outFile, root, userData] = process.argv.slice(-3);
app.setPath('userData', userData);
app.whenReady().then(() => {
  const R = { electron: process.versions.electron, slide: {} };
  try {
    const cp = require(path.join(root, 'src', 'core', 'clippaste'));
    const has = f => { try { return clipboard.has(String(f)); } catch { return false; } };
    const buf = f => { try { const b = clipboard.readBuffer(String(f)); return b && b.length ? b : null; } catch { return null; } };
    R.formats = clipboard.availableFormats();
    for (const f of cp.SLIDE_FORMATS.concat(['WorshipPresenter No Such Format 7f3a91'])) {
      const b = buf(f);
      R.slide[f] = { has: has(f), bytes: b ? b.length : 0, head: b ? b.subarray(0, 8).toString('hex') : '', pptx: b ? cp.looksLikePptx(b) : false, sha1: b ? crypto.createHash('sha1').update(b).digest('hex').slice(0, 12) : '' };
    }
    R.probe = cp.probeSlides({ clipboardHas: has, clipboardBuffer: buf });
    const img = clipboard.readImage();
    R.image = img.isEmpty() ? null : img.getSize();
    R.text = clipboard.readText().length;
    R.html = clipboard.readHTML().length;
  } catch (e) { R.error = String((e && e.stack) || e); }
  fs.writeFileSync(outFile, JSON.stringify(R));
  app.quit();
});
`);
  const env = { ...process.env };
  delete env.ELECTRON_RUN_AS_NODE;
  const code = await new Promise(res => {
    const c = spawn(exe, [script, outFile, ROOT, path.join(BASE, 'electron-probe')], { cwd: ROOT, env, stdio: 'ignore' });
    const k = setTimeout(() => { c.kill(); res('시간 넘김'); }, 40000);
    c.on('exit', x => { clearTimeout(k); res(x); });
    c.on('error', e => { clearTimeout(k); res('실행 실패 ' + e.message); });
  });
  if (!fs.existsSync(outFile)) { say(`  결과 파일이 없음 (Electron 종료 ${code})`); return; }
  const R = JSON.parse(fs.readFileSync(outFile, 'utf8'));
  say(`  Electron ${R.electron} · availableFormats ${JSON.stringify(R.formats)}`);
  if (R.error) say('  오류: ' + firstLines(R.error));
  for (const [f, v] of Object.entries(R.slide || {})) say(`  '${f}': has=${v.has} · readBuffer ${v.bytes}바이트${v.bytes ? ` (처음 ${v.head}) · looksLikePptx=${v.pptx} · sha1 ${v.sha1}${dumpSha ? (v.sha1 === dumpSha ? ' = .NET 으로 읽은 것과 같음' : ' ≠ .NET 으로 읽은 것(' + dumpSha + ')') : ''}` : ''}`);
  say(`  probeSlides → ${JSON.stringify(R.probe)}   (slides true 여야 Ctrl+V 가 바로 슬라이드로 넣는다)`);
  say(`  같이 올라온 것: 그림 ${R.image ? R.image.width + '×' + R.image.height : '없음'} · 글 ${R.text}자 · HTML ${R.html}자`);
}

/** ①-4 PS_CLIP_PASTE 그대로 */
async function stepPaste(tag) {
  head(`①-4 PS_CLIP_PASTE (코드 그대로) [${tag}]`);
  const cp = core('clippaste');
  const file = path.join(OUT, `${tag}-paste.pptx`);
  fs.rmSync(file, { force: true });
  const r = await ps(cp.PS_CLIP_PASTE, { WC_PPT_FILE: file }, { timeout: 120000 });
  say(`  출력: ${firstLines(r.out) || '(없음)'}${r.ok ? '' : ' · 오류: ' + firstLines(r.err)}`);
  if (!fs.existsSync(file)) { say('  저장된 파일이 없음'); return null; }
  const I = pptxInfo(fs.readFileSync(file));
  sayInfo('저장된 파일', I);
  if (I.ok) {
    const o = await ps(PS_OPEN, { WC_FILE: file, WC_OUT: OUT, WC_TAG: `${tag}-paste` }, { sta: false });
    say(`  PowerPoint 로 열기: ${firstLines(o.out, 6) || firstLines(o.err)}`);
  }
  return { file, info: I };
}

function compareRound(tag, title, h) {
  say(`  원본: ${h.info.join(' / ')}`);
  for (const k of [1, 2]) {
    sayDiff(`${k}장 원본 ↔ 묶음 그대로(①-2)`, path.join(OUT, `${tag}-src-${k}.png`), path.join(OUT, `${tag}-dump-${k}.png`));
    sayDiff(`${k}장 원본 ↔ PowerPoint 에 붙여 넣은 것(①-4)`, path.join(OUT, `${tag}-src-${k}.png`), path.join(OUT, `${tag}-paste-${k}.png`));
  }
  html.push(`<h2>${esc(title)}</h2>`);
  for (const k of [1, 2]) {
    const cells = [['원본', `${tag}-src-${k}.png`], ['묶음 그대로 (①-2)', `${tag}-dump-${k}.png`], ['PowerPoint 에 붙여 넣은 것 (①-4)', `${tag}-paste-${k}.png`]].filter(c => fs.existsSync(path.join(OUT, c[1])));
    html.push('<div class="row">' + cells.map(c => fig(`${k}장 · ${c[0]}`, c[1])).join('') + '</div>');
  }
}

/** ② cNvPr id = Shape.Id ? */
async function stepIds(fixture) {
  head(`②-1 XML 의 cNvPr id ↔ COM 의 Shape.Id [${fixture}]`);
  const file = path.join(OUT, 'ids-' + fixture);
  fs.copyFileSync(path.join(ROOT, 'tests', 'fixtures', fixture), file);
  const I = pptxInfo(fs.readFileSync(file));
  const r = await ps(PS_SHAPES, { WC_FILE: file }, { sta: false });
  const rows = r.out.split(/\r?\n/).map(l => l.split('\t'));
  if (!rows.some(x => x[0] === 'SH')) { say('  COM 으로 읽지 못함: ' + (firstLines(r.out) || firstLines(r.err))); return; }
  let bad = 0, total = 0;
  I.slides.forEach((s, i) => {
    const com = rows.filter(x => x[0] === 'SH' && +x[1] === i + 1).map(x => ({ depth: +x[2], id: x[3], type: x[4], hide: x[5], name: x.slice(6).join('\t') }));
    const key = a => a.map(x => x.id + ':' + x.name).sort().join(' | ');
    const same = key(com) === key(s.shapes);
    total += com.length;
    if (!same) bad++;
    say(`  ${i + 1}장${s.hidden ? ' (숨김)' : ''}: ${same ? `같음 (${com.length}개)` : '다름'}`);
    if (!same) { say(`     XML: ${key(s.shapes)}`); say(`     COM: ${key(com)}`); }
    const inGroup = com.filter(x => x.depth > 0), noHide = com.filter(x => x.hide !== 'ok');
    if (inGroup.length) say(`     그룹 안 도형: ${inGroup.map(x => `${x.id}:${x.name}(숨기기 ${x.hide})`).join(' | ')}`);
    if (noHide.length) say(`     Visible=0 이 안 되는 도형: ${noHide.map(x => `${x.id}:${x.name} type ${x.type} → ${x.hide}`).join(' | ')}`);
  });
  say(`  → ${bad ? `다른 슬라이드 ${bad}장` : `모두 같음 (도형 ${total}개)`}`);
}

/** ② PS_BG 그대로: 코드가 만드는 hide 로 배경 그림을 뽑아 원래 슬라이드와 견준다 */
async function stepBg(fixture) {
  head(`②-2 PS_BG (코드 그대로) — 꺼낸 요소가 배경 그림에서 빠지는가 [${fixture}]`);
  const tag = 'bg-' + fixture.replace(/\.pptx$/, '');
  const dir = path.join(OUT, tag);
  fs.rmSync(dir, { recursive: true, force: true });
  fs.mkdirSync(dir, { recursive: true });
  const file = path.join(OUT, tag + '.pptx');
  fs.copyFileSync(path.join(ROOT, 'tests', 'fixtures', fixture), file);
  let raw;
  try { raw = (await core('pptxbuild').build(file, dir, { full: false, log: m => say('  (파서) ' + m) })).raw; } catch (e) { say('  요소를 꺼내지 못함: ' + e.message); return; }
  say(`  엔진 ${raw.engine} · 슬라이드 ${raw.w}×${raw.h}pt · hide = ${JSON.stringify(raw.hide)}`);
  const pr = core('pptrender');
  try { const m = await pr.exportBgWithPowerPoint(file, dir, raw.hide); say(`  PS_BG: OK ${m.n}장`); } catch (e) { say('  PS_BG 실패: ' + e.message); return; }
  try { const m = await pr.exportWithPowerPoint(file, dir); say(`  원래 슬라이드(PS): ${m.files.length}장 ${m.width}×${m.height}`); } catch (e) { say('  원래 슬라이드 내보내기 실패: ' + e.message); return; }
  html.push(`<h2>② 배경 그림 — ${esc(fixture)} (분홍 테두리 = 꺼낸 요소의 자리)</h2>`);
  raw.slides.forEach((els, i) => {
    const n = String(i + 1).padStart(3, '0');
    const full = path.join(dir, `slide-${n}.png`), bg = path.join(dir, `slide-${n}-bg.png`);
    const rects = (els || []).map(e => ({ l: e.l / raw.w, t: e.t / raw.h, w: e.w / raw.w, h: e.h / raw.h, label: `${e.kind} ${e.id}` }));
    try {
      const d = diffPng(full, bg, rects);
      if (!d.same) say(`  ${i + 1}장: ${d.note}`);
      else {
        say(`  ${i + 1}장: 요소 ${rects.length}개 (숨길 id ${JSON.stringify(raw.hide[String(i + 1)] || [])}) · 요소 자리 밖에서 달라진 픽셀 ${d.outside}% (0 에 가까워야 함)`);
        rects.forEach((r, k) => say(`     ${r.label} 자리(${Math.round(els[k].l)},${Math.round(els[k].t)} ${Math.round(els[k].w)}×${Math.round(els[k].h)}pt): 달라진 픽셀 ${d.rects[k]}%${d.rects[k] === 0 ? '  ← 안 빠졌거나 배경과 같은 색' : ''}`));
      }
    } catch (e) { say(`  ${i + 1}장: 견주지 못함 — ${e.message}`); }
    const pct = rects.map(r => ({ l: r.l * 100, t: r.t * 100, w: r.w * 100, h: r.h * 100, label: r.label }));
    const rel = f => path.posix.join(tag, path.basename(f));
    html.push('<div class="row">' + [fs.existsSync(full) && fig(`${i + 1}장 · 원래 슬라이드 + 요소 자리`, rel(full), pct), fs.existsSync(bg) && fig(`${i + 1}장 · 배경 그림 (요소를 숨긴 것)`, rel(bg)), fs.existsSync(bg) && fig(`${i + 1}장 · 배경 그림 + 요소 자리`, rel(bg), pct)].filter(Boolean).join('') + '</div>');
  });
}

function prepareData() {
  fs.mkdirSync(DATA, { recursive: true });
  const db = path.join(DATA, 'bible.db');
  if (!fs.existsSync(db)) fs.copyFileSync(path.join(ROOT, 'data', 'bible.db'), db);
  const st = path.join(DATA, 'settings.json');
  if (!fs.existsSync(st)) fs.writeFileSync(st, JSON.stringify({ port: PORT, output: { autoOnSend: false, enabled: false }, previewMode: true, remote: { enabled: false, pin: '482916', port: PORT + 1, host: '127.0.0.1' }, telemetry: false }));
}

const NEXT = `
이제 손으로 해 볼 것 (5분):
  1) node tools/ppt-check.js app     ← 임시 데이터 폴더 · 포트 ${PORT} 로 프로그램이 뜬다 (쓰던 프로그램은 그대로)
  2) PowerPoint 에서 tests\\fixtures\\elements.pptx 를 열고, 왼쪽 슬라이드 목록에서 1·2장을 골라 Ctrl+C
     → (바로) node tools/ppt-check.js clip   ← 손으로 복사했을 때의 클립보드를 적어 둔다
     → 뜬 프로그램의 큐시트를 한 번 누르고 Ctrl+V  : 찬송가·PPT 순서로 들어가는지, 그림 한 장으로 들어가는지
     → 한 번 더 복사한 뒤 Ctrl+Shift+V → 'PowerPoint 슬라이드' 를 골랐을 때는?
  3) tests\\fixtures\\elements.pptx 와 chart-table.pptx 를 큐시트에 끌어다 놓고, 순서를 골라 편집칸의
     '글·그림 따로 고치기…' → 요소가 둘로 겹쳐 보이지 않는지 · 꺼낸 글상자를 옮겼을 때 원래 자리에 글이 남지 않는지 → '원래대로'
  4) node tools/ppt-check.js log     ← 프로그램이 남긴 기록을 모은다 (프로그램은 그 뒤에 닫아도 된다)
  5) ${path.join(OUT, 'result.txt')} 와 result-clip.txt · result-log.txt 의 내용을 Claude 에게 붙여 준다
     (그림은 ${path.join(OUT, 'compare.html')} 를 브라우저로 열어 눈으로)
`;

async function runAll() {
  if (!(await preflight())) return;
  say('클립보드 내용이 바뀝니다. PowerPoint 창이 잠깐 뜹니다. 5초 뒤 시작 (그만두려면 Ctrl+C).');
  await wait(5000);

  head('원본 16:9 (elements.pptx 의 1·2장을 복사)');
  let h = await hold('a', 'elements.pptx', false);
  if (h) {
    try {
      say('  ' + h.info.join('\n  '));
      await stepFormats();
      const dump = await stepDump('a');
      await stepElectron(dump && dump.sha);
      await stepPaste('a');
      head('①-4 견주기 [16:9 원본]');
      compareRound('a', '① 붙여 넣기 — 16:9 원본', h);
    } catch (e) { say('  중간에 멈춤: ' + ((e && e.stack) || e)); } finally { await h.stop(); }
  }

  head('원본 4:3 (같은 파일을 4:3 으로 바꿔 1·2장을 복사)');
  h = await hold('b', 'elements.pptx', true);
  if (h) {
    try {
      say('  ' + h.info.join('\n  '));
      const dump = await stepDump('b');
      const paste = await stepPaste('b');
      head('①-4 견주기 [4:3 원본]');
      if (paste && paste.info.ok) say(`  붙여 넣어 저장한 파일의 슬라이드 비율 ${paste.info.ratio.toFixed(3)}:1 (4:3 = 1.333 · 16:9 = 1.778)`);
      if (dump && dump.info.ok) say(`  묶음 그대로 저장한 파일의 슬라이드 비율 ${dump.info.ratio.toFixed(3)}:1`);
      compareRound('b', '① 붙여 넣기 — 4:3 원본', h);
    } catch (e) { say('  중간에 멈춤: ' + ((e && e.stack) || e)); } finally { await h.stop(); }
  }

  for (const f of ['elements.pptx', 'chart-table.pptx']) {
    try { await stepIds(f); } catch (e) { say('  중간에 멈춤: ' + ((e && e.stack) || e)); }
    try { await stepBg(f); } catch (e) { say('  중간에 멈춤: ' + ((e && e.stack) || e)); }
  }
  try { prepareData(); } catch (e) { say('임시 데이터 폴더를 만들지 못함: ' + e.message); }
  saveResults('result.txt');
  console.log(`\n결과: ${path.join(OUT, 'result.txt')}\n그림: ${path.join(OUT, 'compare.html')}`);
  console.log(NEXT);
}

async function runClip() {
  if (!(await preflight())) return;
  say('(지금 클립보드로 — 손으로 복사한 것)');
  const names = await stepFormats();
  const dump = names.length ? await stepDump('c') : null;
  await stepElectron(dump && dump.sha);
  if (names.length) await stepPaste('c'); // PowerPoint 가 떠 있고 사람이 복사해 둔 상태에서의 ①-4
  saveResults('result-clip.txt');
  console.log(`\n결과: ${path.join(OUT, 'result-clip.txt')}`);
}

function runApp() {
  if (process.platform !== 'win32') { console.log('윈도우에서만 됩니다.'); return; }
  prepareData();
  const env = { ...process.env };
  delete env.ELECTRON_RUN_AS_NODE;
  let exe;
  try { exe = require(path.join(ROOT, 'node_modules', 'electron')); } catch { console.log('electron 이 없습니다 — 저장소 폴더에서 npm install 을 먼저 해 주세요.'); return; }
  spawn(exe, ['.', '--data', DATA], { cwd: ROOT, env, detached: true, stdio: 'ignore' }).unref();
  console.log(`워십프리젠터를 띄웠습니다 — 데이터 폴더 ${DATA} · 포트 ${PORT} (쓰던 프로그램과 따로).`);
  console.log(NEXT);
}

function runLog() {
  fs.mkdirSync(OUT, { recursive: true });
  say(`워십프리젠터 PowerPoint 확인 (프로그램 기록) · ${new Date().toLocaleString()} · 데이터 폴더 ${DATA}`);
  head('기록 (logs) 가운데 붙여 넣기 · PowerPoint');
  const logDir = path.join(DATA, 'logs');
  const logs = fs.existsSync(logDir) ? fs.readdirSync(logDir).filter(f => f.endsWith('.log')).sort() : [];
  const hits = [];
  for (const f of logs) for (const l of fs.readFileSync(path.join(logDir, f), 'utf8').split(/\r?\n/)) if (/붙여 넣기|PowerPoint|배경 그림|\[(WARN|ERROR)\]/.test(l)) hits.push(l);
  say(hits.length ? hits.slice(-80).map(l => '  ' + l).join('\n') : '  (해당 줄 없음)');

  head('붙여 넣은 슬라이드 파일 (pasted)');
  const pd = path.join(DATA, 'pasted');
  const pasted = fs.existsSync(pd) ? fs.readdirSync(pd).filter(f => /\.pptx$/i.test(f)).sort() : [];
  if (!pasted.length) say('  (없음 — 슬라이드로 들어가지 않았다)');
  for (const f of pasted) { try { sayInfo(f, pptxInfo(fs.readFileSync(path.join(pd, f)))); } catch (e) { say(`  ${f}: ${e.message}`); } }

  head('요소 따로 고치기 (hymn-cache 의 layers.json)');
  const cd = path.join(DATA, 'hymn-cache');
  const keys = fs.existsSync(cd) ? fs.readdirSync(cd).filter(k => fs.existsSync(path.join(cd, k, 'layers.json'))) : [];
  if (!keys.length) say("  (없음 — '글·그림 따로 고치기…' 를 누르지 않았다)");
  for (const k of keys) {
    const dir = path.join(cd, k);
    let raw;
    try { raw = JSON.parse(fs.readFileSync(path.join(dir, 'layers.json'), 'utf8').replace(/^﻿/, '')); } catch (e) { say(`  ${k}: layers.json 을 읽지 못함 ${e.message}`); continue; }
    const files = fs.readdirSync(dir);
    say(`  ${k}: 엔진 ${raw.engine} · bgFrom ${raw.bgFrom || '(없음 = 직접 그린 SVG 배경)'} · bgs ${JSON.stringify(raw.bgs)}`);
    say(`     hide ${JSON.stringify(raw.hide)} · 폴더: 원래 그림 ${files.filter(f => /^slide-\d{3}\.(png|svg)$/.test(f)).length}장 · 배경 png ${files.filter(f => /-bg\.png$/.test(f)).length} · 배경 svg ${files.filter(f => /-bg\.svg$/.test(f)).length}`);
    html.push(`<h2>프로그램이 만든 배경 — ${esc(k)}</h2>`);
    (raw.slides || []).forEach((els, i) => {
      const n = String(i + 1).padStart(3, '0');
      const full = path.join(dir, `slide-${n}.png`), bg = path.join(dir, `slide-${n}-bg.png`);
      const rects = (els || []).map(e => ({ l: e.l / raw.w, t: e.t / raw.h, w: e.w / raw.w, h: e.h / raw.h, label: `${e.kind} ${e.id}` }));
      if (fs.existsSync(full) && fs.existsSync(bg)) {
        try {
          const d = diffPng(full, bg, rects);
          say(d.same ? `     ${i + 1}장: 요소 자리 밖에서 달라진 픽셀 ${d.outside}% · 요소 자리마다 ${rects.map((r, j) => `${r.label} ${d.rects[j]}%`).join(' | ')}` : `     ${i + 1}장: ${d.note}`);
        } catch (e) { say(`     ${i + 1}장: 견주지 못함 — ${e.message}`); }
      } else say(`     ${i + 1}장: ${fs.existsSync(full) ? '' : 'slide-' + n + '.png 없음 '}${fs.existsSync(bg) ? '' : 'slide-' + n + '-bg.png 없음'}`);
      const pct = rects.map(r => ({ l: r.l * 100, t: r.t * 100, w: r.w * 100, h: r.h * 100, label: r.label }));
      html.push('<div class="row">' + [fs.existsSync(full) && fig(`${i + 1}장 · 원래 슬라이드 + 요소 자리`, full, pct), fs.existsSync(bg) && fig(`${i + 1}장 · 배경 그림`, bg)].filter(Boolean).join('') + '</div>');
    });
  }
  saveResults('result-log.txt');
  console.log(`\n결과: ${path.join(OUT, 'result-log.txt')}${html.length ? '\n그림: ' + path.join(OUT, 'compare-log.html') : ''}`);
}

if (require.main === module) {
  const mode = process.argv[2] || '';
  const run = mode === 'clip' ? runClip : mode === 'app' ? runApp : mode === 'log' ? runLog : runAll;
  Promise.resolve().then(run).catch(e => { console.error(e); try { saveResults('result-error.txt'); } catch { /* 그만 */ } process.exit(1); });
}

module.exports = { pptxInfo, xmlShapes, readPng, diffPng, decode, runAll, runClip, runLog, PS_HOLD, PS_FORMATS, PS_OPEN, PS_SHAPES };
