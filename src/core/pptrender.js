'use strict';
// PPT 슬라이드를 '그대로' 그림으로 만든다
// - PowerPoint 가 설치돼 있으면 슬라이드마다 PNG 로 내보낸다 (.ppt .pps .pptx .ppsx 모두, 화면 그대로)
// - 없으면 .pptx/.ppsx 는 압축을 풀어 슬라이드 글자와 배경 그림만 꺼낸다

const fs = require('fs');
const path = require('path');
const zlib = require('zlib');
const { execFile } = require('child_process');

let ppAvailable = null;
/** PowerPoint 설치 여부 (레지스트리의 PowerPoint.Application) */
function hasPowerPoint() {
  if (ppAvailable !== null) return Promise.resolve(ppAvailable);
  if (process.platform !== 'win32') return Promise.resolve(ppAvailable = false);
  return new Promise(res => execFile('reg', ['query', 'HKCR\\PowerPoint.Application\\CurVer'], { windowsHide: true }, err => res(ppAvailable = !err)));
}

const PS = `
$ErrorActionPreference = 'Stop'
$pp = New-Object -ComObject PowerPoint.Application
try {
  $p = $pp.Presentations.Open($env:WC_PPT_FILE, -1, 0, 0)
  $sw = $p.PageSetup.SlideWidth; $sh = $p.PageSetup.SlideHeight
  # (PowerShell 변수는 대소문자를 가리지 않는다)
  $outW = 1920; $outH = [int](1920 * $sh / $sw)
  $n = 0
  $texts = New-Object System.Collections.ArrayList
  # 슬라이드 글자: 글상자(그룹 안 포함)를 위에서 아래 순서로. 쪽 번호·바닥글·날짜 자리는 뺀다
  function Get-Texts($shapes) {
    $out = @()
    foreach ($sh in $shapes) {
      try {
        if ($sh.Type -eq 6) { $out += Get-Texts $sh.GroupItems; continue }
        if ($sh.Type -eq 14) { $pt = $sh.PlaceholderFormat.Type; if ($pt -eq 13 -or $pt -eq 15 -or $pt -eq 16) { continue } }
        if ($sh.HasTextFrame -and $sh.TextFrame.HasText) {
          $out += [pscustomobject]@{ top = [double]$sh.Top; left = [double]$sh.Left; text = [string]$sh.TextFrame.TextRange.Text }
        }
      } catch { }
    }
    return $out
  }
  foreach ($s in $p.Slides) {
    if ($s.SlideShowTransition.Hidden) { continue }
    $n++
    $s.Export((Join-Path $env:WC_PPT_OUT ('slide-{0:D3}.png' -f $n)), 'PNG', $outW, $outH)
    $parts = @(Get-Texts $s.Shapes | Sort-Object top, left | ForEach-Object { $_.text })
    [void]$texts.Add(($parts -join [char]10))
  }
  $json = ConvertTo-Json -InputObject @($texts) -Compress
  [IO.File]::WriteAllText((Join-Path $env:WC_PPT_OUT 'texts.json'), $json, (New-Object Text.UTF8Encoding $false))
  $p.Close()
  Write-Output "OK $n $outW $outH"
} finally {
  if ($pp.Presentations.Count -eq 0) { $pp.Quit() }
}
`;

// PPT 요소 따로 고치기: 슬라이드의 요소(글상자 · 그림 · 도형 · 표 · 차트)를 하나씩 꺼낸다 → layers.json
//   글만 있는 상자 → 글상자 { kind:'text', 위치·크기(포인트) · 글 · 대표 글꼴(가장 긴 글 조각) · 정렬 · 여백 · 줄 간격 }
//   그림·도형·표·차트 → 요소 그림 { kind:'image', file: 'slide-001-e01.png' } (투명 배경 PNG, Shape.Export)
//     도형 안의 글(표·차트·SmartArt 빼고)은 글을 숨기고 그림으로 꺼낸 뒤 그 위에 글상자로
//   배경 = 꺼낸 요소를 모두 숨긴 슬라이드 그림 (slide-001-bg.png: 슬라이드 배경 · 마스터의 장식 · 꺼내지 않은 요소)
// - 돌린 요소 · 세로 쓰기 · WordArt · 쪽 번호·바닥글·날짜 자리는 배경 그림에 그대로 둔다
// - 요소 그림을 못 만들면(Shape.Export 가 안 되는 PowerPoint) 그 요소는 배경에 두고 글만 글상자로 (①)
// - 파일은 저장하지 않는다 (읽기 전용으로 열고 저장 안 함 표시 후 닫음)
const PS_LAYERS = `
$ErrorActionPreference = 'Stop'
$pp = New-Object -ComObject PowerPoint.Application
try {
  $p = $pp.Presentations.Open($env:WC_PPT_FILE, -1, 0, 0)
  $slideW = $p.PageSetup.SlideWidth; $slideH = $p.PageSetup.SlideHeight
  $outW = 1920; $outH = [int](1920 * $slideH / $slideW)
  function Get-Flat($shapes) {
    $out = New-Object System.Collections.ArrayList
    foreach ($x in $shapes) {
      try {
        if ($x.Visible -eq 0) { continue }
        if ($x.Type -eq 6) { if ($x.Rotation -eq 0) { foreach ($y in (Get-Flat $x.GroupItems)) { [void]$out.Add($y) } }; continue }
        [void]$out.Add($x)
      } catch { }
    }
    return ,$out
  }
  function Get-MainRun($tr) {
    $best = $tr; $bl = -1
    $cnt = [Math]::Min([int]$tr.Runs().Count, 60)
    for ($i = 1; $i -le $cnt; $i++) {
      $r = $tr.Runs($i)
      $l = ([string]$r.Text).Trim().Length
      if ($l -gt $bl) { $best = $r; $bl = $l }
    }
    return $best
  }
  function Get-TextBox($x) {
    $tf = $x.TextFrame
    $tr = $tf.TextRange
    $paras = New-Object System.Collections.ArrayList
    $pc = [Math]::Min([int]$tr.Paragraphs().Count, 200)
    for ($i = 1; $i -le $pc; $i++) {
      $pg = $tr.Paragraphs($i)
      $t = ([string]$pg.Text).TrimEnd([char]13)
      try { if ($pg.ParagraphFormat.Bullet.Visible -eq -1 -and $t.Trim()) { if ($pg.ParagraphFormat.Bullet.Type -eq 2) { $t = [string]$i + '. ' + $t } else { $t = [char]0x2022 + ' ' + $t } } } catch { }
      [void]$paras.Add($t)
    }
    $f = (Get-MainRun $tr).Font
    $lh = 0
    try { if ($tr.Paragraphs(1).ParagraphFormat.LineRuleWithin -eq -1) { $lh = [double]$tr.Paragraphs(1).ParagraphFormat.SpaceWithin } } catch { }
    return [pscustomobject]@{
      kind = 'text'; l = [double]$x.Left; t = [double]$x.Top; w = [double]$x.Width; h = [double]$x.Height
      text = ($paras -join [char]10)
      size = [double]$f.Size; bold = [int]$f.Bold; italic = [int]$f.Italic; color = [int]$f.Color.RGB
      font = [string]$f.Name; fontEa = [string]$f.NameFarEast
      align = [int]$tr.Paragraphs(1).ParagraphFormat.Alignment; anchor = [int]$tf.VerticalAnchor
      ml = [double]$tf.MarginLeft; mr = [double]$tf.MarginRight; mt = [double]$tf.MarginTop; mb = [double]$tf.MarginBottom
      lh = $lh; wrap = [int]$tf.WordWrap
    }
  }
  # 글만 숨긴다 (글자 채우기·윤곽·그림자·빛 · 글머리표). 도형의 색·테두리는 그대로 → 숨겼으면 $true
  function Hide-Text($x) {
    try {
      $f2 = $x.TextFrame2.TextRange.Font
      try { $f2.Fill.Visible = 0 } catch { $f2.Fill.Transparency = 1 }
      try { $f2.Line.Visible = 0 } catch { }
      try { $f2.Shadow.Visible = 0 } catch { }
      try { $f2.Glow.Radius = 0 } catch { }
      try { $x.TextFrame.TextRange.ParagraphFormat.Bullet.Visible = 0 } catch { }
      return $true
    } catch { return $false }
  }
  # 요소 하나를 투명 배경 PNG 로 (슬라이드를 $outW 너비로 내보낼 때와 같은 배율). Shape.Export 는 숨은 메서드라 안 되면 IDispatch 로 직접
  function Export-Shape($x, $path) {
    try { [void]$x.Export($path, 2, $outW, $outH, 1) } catch { }
    if (Test-Path -LiteralPath $path) { return $true }
    try { [void][System.__ComObject].InvokeMember('Export', [Reflection.BindingFlags]::InvokeMethod, $null, $x, @($path, 2, $outW, $outH, 1)) } catch { }
    return (Test-Path -LiteralPath $path)
  }
  $n = 0
  $slides = New-Object System.Collections.ArrayList
  foreach ($s in $p.Slides) {
    if ($s.SlideShowTransition.Hidden) { continue }
    $n++
    $els = New-Object System.Collections.ArrayList
    $m = 0
    foreach ($x in (Get-Flat $s.Shapes)) {
      try {
        if ($x.Type -eq 14) { $pt = $x.PlaceholderFormat.Type; if ($pt -eq 13 -or $pt -eq 15 -or $pt -eq 16) { continue } }
        if (($x.Rotation % 360) -ne 0) { continue }
        $complex = $false
        try { $complex = ($x.HasTable -eq -1) -or ($x.HasChart -eq -1) -or ($x.HasSmartArt -eq -1) } catch { }
        $hasText = $false
        try { $hasText = ($x.Type -ne 15) -and ($x.HasTextFrame -eq -1) -and ($x.TextFrame.HasText -eq -1) -and ($x.TextFrame.Orientation -eq 1) -and (-not $complex) } catch { }
        # 그림 · 연결 그림 · 미디어(포스터) · SVG · OLE 개체 · 차트 · 3D 모델
        $pic = @(13, 11, 16, 28, 7, 10, 3, 30) -contains [int]$x.Type
        try { if ($x.Type -eq 14 -and $x.PlaceholderFormat.ContainedType -eq 13) { $pic = $true } } catch { }
        $fill = $false; $line = $false
        try { $fill = ($x.Fill.Visible -eq -1) } catch { }
        try { $line = ($x.Line.Visible -eq -1) } catch { }
        # 글만 있는 상자: 글상자로 꺼내고 상자째 숨김
        if ($hasText -and -not $pic -and -not $fill -and -not $line) {
          $box = Get-TextBox $x
          $x.Visible = 0
          [void]$els.Add($box)
          continue
        }
        # 보이는 것이 없는 자리(빈 개체 틀 · 채우기·선 없는 도형)는 그대로
        if (-not $pic -and -not $fill -and -not $line -and -not $complex) { continue }
        # 그림·도형·표·차트: 요소 그림으로 (도형 안 글은 숨기고 글상자로)
        $box = $null
        if ($hasText) { $box = Get-TextBox $x; if (-not (Hide-Text $x)) { $box = $null } }
        $m++
        $file = 'slide-{0:D3}-e{1:D2}.png' -f $n, $m
        if (Export-Shape $x (Join-Path $env:WC_PPT_OUT $file)) {
          [void]$els.Add([pscustomobject]@{ kind = 'image'; l = [double]$x.Left; t = [double]$x.Top; w = [double]$x.Width; h = [double]$x.Height; file = $file })
          if ($box) { [void]$els.Add($box) }
          $x.Visible = 0
        } elseif ($box) {
          [void]$els.Add($box)
        }
      } catch { }
    }
    $s.Export((Join-Path $env:WC_PPT_OUT ('slide-{0:D3}-bg.png' -f $n)), 'PNG', $outW, $outH)
    [void]$slides.Add($els)
  }
  $json = ConvertTo-Json -InputObject ([pscustomobject]@{ v = 2; w = [double]$slideW; h = [double]$slideH; slides = $slides }) -Depth 6 -Compress
  [IO.File]::WriteAllText((Join-Path $env:WC_PPT_OUT 'layers.json'), $json, (New-Object Text.UTF8Encoding $false))
  $p.Saved = -1
  $p.Close()
  Write-Output "OK $n $outW $outH"
} finally {
  if ($pp.Presentations.Count -eq 0) { $pp.Quit() }
}
`;

/** 큰 PPT 는 오래 걸린다: 1MB 마다 4초 더 (2분 ~ 30분) */
function pptTimeout(file) {
  let size = 0;
  try { size = fs.statSync(file).size; } catch { /* 없으면 PowerPoint 가 알린다 */ }
  return Math.max(120000, Math.min(1800000, 60000 + Math.round(size / 1048576) * 4000));
}

/** PowerShell 스크립트로 PowerPoint 를 부른다 → stdout 의 'OK n w h' */
function runPowerPoint(script, file, outDir) {
  fs.mkdirSync(outDir, { recursive: true });
  const enc = Buffer.from(script, 'utf16le').toString('base64');
  const timeout = pptTimeout(file);
  return new Promise((resolve, reject) => {
    execFile('powershell.exe', ['-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass', '-EncodedCommand', enc], {
      windowsHide: true, timeout,
      env: { ...process.env, WC_PPT_FILE: path.resolve(file), WC_PPT_OUT: path.resolve(outDir) },
    }, (err, stdout, stderr) => {
      const m = String(stdout).match(/OK (\d+) (\d+) (\d+)/);
      if (err || !m) {
        const msg = String(stderr || (err && err.message) || '');
        if (/password|암호/i.test(msg)) return reject(new Error('암호가 걸린 파일이라 열 수 없습니다. PowerPoint 에서 암호를 풀고 저장해 주세요.'));
        if (err && err.killed) return reject(new Error(`PowerPoint 로 슬라이드를 만드는 데 ${Math.round(timeout / 60000)}분을 넘겼습니다. 파일이 너무 크면 PowerPoint 에서 '미디어 압축'으로 줄이거나 나눠 저장해 보세요.`));
        return reject(new Error('PowerPoint 로 슬라이드를 만들지 못했습니다. ' + (msg.split('\n').find(l => l.trim()) || '')));
      }
      resolve({ n: +m[1], width: +m[2], height: +m[3] });
    });
  });
}

/** PowerPoint 로 슬라이드를 PNG 로 내보낸다 → ['slide-001.png', …] */
async function exportWithPowerPoint(file, outDir) {
  const m = await runPowerPoint(PS, file, outDir);
  const files = fs.readdirSync(outDir).filter(f => /^slide-\d{3}\.png$/.test(f)).sort();
  // 슬라이드 글자 (PowerPoint 줄바꿈 \r · 세로탭 정리)
  let texts = [];
  try { texts = JSON.parse(fs.readFileSync(path.join(outDir, 'texts.json'), 'utf8').replace(/^﻿/, '')); } catch { /* 글자 없음 */ }
  if (!Array.isArray(texts)) texts = [texts];
  texts = texts.map(t => String(t || '').replace(/\r\n?|\u000b/g, '\n').replace(/\n{3,}/g, '\n\n').trim());
  return { files, texts, width: m.width, height: m.height };
}

/** 요소 따로 고치기용 배경 그림·요소 그림·글상자 정보 → { n, raw: { v, w, h, slides: [[요소…]] } } (layers.json 에도 남는다) */
async function exportLayers(file, outDir) {
  const m = await runPowerPoint(PS_LAYERS, file, outDir);
  const raw = JSON.parse(fs.readFileSync(path.join(outDir, 'layers.json'), 'utf8').replace(/^﻿/, ''));
  return { n: m.n, raw };
}

// ---------------- .pptx (zip) ----------------
function readZip(buf) {
  let eocd = -1;
  for (let i = buf.length - 22; i >= Math.max(0, buf.length - 70000); i--) if (buf.readUInt32LE(i) === 0x06054b50) { eocd = i; break; }
  if (eocd < 0) throw new Error('pptx 파일 형식이 아닙니다.');
  const count = buf.readUInt16LE(eocd + 10);
  let p = buf.readUInt32LE(eocd + 16);
  const entries = new Map();
  for (let k = 0; k < count && buf.readUInt32LE(p) === 0x02014b50; k++) {
    const method = buf.readUInt16LE(p + 10), csize = buf.readUInt32LE(p + 20);
    const nlen = buf.readUInt16LE(p + 28), xlen = buf.readUInt16LE(p + 30), clen = buf.readUInt16LE(p + 32);
    const off = buf.readUInt32LE(p + 42);
    const name = buf.subarray(p + 46, p + 46 + nlen).toString('utf8');
    entries.set(name, { method, csize, off });
    p += 46 + nlen + xlen + clen;
  }
  const get = name => {
    const e = entries.get(name);
    if (!e) return null;
    const start = e.off + 30 + buf.readUInt16LE(e.off + 26) + buf.readUInt16LE(e.off + 28);
    const raw = buf.subarray(start, start + e.csize);
    return e.method === 8 ? zlib.inflateRawSync(raw) : raw;
  };
  return { names: [...entries.keys()], get };
}

const xmlText = s => s.replace(/&lt;/g, '<').replace(/&gt;/g, '>').replace(/&quot;/g, '"').replace(/&apos;/g, "'").replace(/&amp;/g, '&');

/** .pptx → { background, texts[][] } (PowerPoint 가 없을 때) */
function extractPptx(file) {
  const z = readZip(fs.readFileSync(file));
  const slides = z.names.filter(n => /^ppt\/slides\/slide\d+\.xml$/.test(n)).sort((a, b) => +a.match(/\d+/)[0] - +b.match(/\d+/)[0]);
  const texts = [];
  const used = new Map();
  for (const s of slides) {
    const xml = z.get(s).toString('utf8');
    const paras = [...xml.matchAll(/<a:p>([\s\S]*?)<\/a:p>/g)].map(m => [...m[1].matchAll(/<a:t>([^<]*)<\/a:t>/g)].map(t => xmlText(t[1])).join('')).filter(t => t.trim());
    texts.push(paras);
    const rels = z.get(s.replace('slides/', 'slides/_rels/') + '.rels');
    if (rels) for (const m of rels.toString('utf8').matchAll(/Target="\.\.\/media\/([^"]+\.(png|jpe?g))"/gi)) used.set(m[1], (used.get(m[1]) || 0) + 1);
  }
  // 슬라이드에 그림이 없으면 레이아웃·마스터(배경이 보통 여기에)
  if (!used.size) {
    for (const n of z.names.filter(x => /^ppt\/(slideLayouts|slideMasters)\/_rels\/.+\.rels$/.test(x))) {
      for (const m of z.get(n).toString('utf8').matchAll(/Target="\.\.\/media\/([^"]+\.(png|jpe?g))"/gi)) used.set(m[1], (used.get(m[1]) || 0) + 1);
    }
  }
  // 가장 많은 슬라이드에 쓰인 그림 = 배경
  let background = null;
  const best = [...used.entries()].sort((a, b) => b[1] - a[1])[0];
  if (best) background = { ext: /png$/i.test(best[0]) ? 'png' : 'jpg', data: z.get('ppt/media/' + best[0]) };
  return { background, scores: [], texts };
}

module.exports = { hasPowerPoint, exportWithPowerPoint, exportLayers, extractPptx, readZip, PS, PS_LAYERS };
