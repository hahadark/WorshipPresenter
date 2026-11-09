# PPT 요소 따로 꺼내기 스크립트(pptrender.js PS_LAYERS)를 가짜 PowerPoint 객체로 돌려 본다 (tests/core.test.js 가 부름)
#   실제 PowerPoint 없이 스크립트의 흐름만 점검: 글만 있는 상자 · 그룹 안 글상자 · 그림 · 글이 든 도형 · 그림으로 못 꺼내는 도형
#   · 숨긴 슬라이드 · 돌린 요소 · 바닥글 자리 · 글머리표 · 글 숨기기 · JSON 모양
#   $env:WC_PPT_SCRIPT = 점검할 스크립트 파일, $env:WC_PPT_OUT = 결과 폴더
$ErrorActionPreference = 'Stop'

function New-Para($text, $align, $bullet) {
  $pf = [pscustomobject]@{ Alignment = $align; LineRuleWithin = -1; SpaceWithin = 1.0; Bullet = [pscustomobject]@{ Visible = $bullet; Type = 1 } }
  return [pscustomobject]@{ Text = $text; ParagraphFormat = $pf }
}
function New-Run($text, $size, $bold, $rgb) {
  $font = [pscustomobject]@{ Size = $size; Bold = $bold; Italic = 0; Color = [pscustomobject]@{ RGB = $rgb }; Name = 'Arial'; NameFarEast = '맑은 고딕' }
  return [pscustomobject]@{ Text = $text; Font = $font }
}
# 여러 번째를 고르는 PowerPoint 모양 함수: Runs() → .Count, Runs(i) → i 번째 (1부터)
function Add-Indexer($obj, $name, $list) {
  $obj | Add-Member -MemberType NoteProperty -Name ('_' + $name) -Value $list
  $sb = [scriptblock]::Create("param(`$i) `$l = `$this._$name; if (`$null -eq `$i) { return [pscustomobject]@{ Count = `$l.Count } }; return `$l[`$i - 1]")
  $obj | Add-Member -MemberType ScriptMethod -Name $name -Value $sb
}
# 요소 그림 내보내기: 'ok' = 파일을 쓴다 · 'fail' = 숨은 메서드가 없는 PowerPoint 처럼 실패
function Add-Export($sh, $how) {
  if ($how -eq 'ok') { $sh | Add-Member -MemberType ScriptMethod -Name Export -Value { param($path, $fmt, $w, $h, $mode) [IO.File]::WriteAllText($path, "PNG shape $fmt $w x $h mode $mode") } }
  else { $sh | Add-Member -MemberType ScriptMethod -Name Export -Value { param($path, $fmt, $w, $h, $mode) throw 'Export 없음' } }
}
function New-Shape($type, $left, $top, $w, $h, $fill, $line, $opts) {
  $sh = [pscustomobject]@{ Type = $type; Visible = -1; Rotation = 0; HasTextFrame = 0; Left = $left; Top = $top; Width = $w; Height = $h
    Fill = [pscustomobject]@{ Visible = $fill }; Line = [pscustomobject]@{ Visible = $line }; HasTable = 0; HasChart = 0; HasSmartArt = 0
    PlaceholderFormat = [pscustomobject]@{ Type = 2; ContainedType = 1 } }
  if ($opts) { foreach ($k in $opts.Keys) { $sh.$k = $opts[$k] } }
  return $sh
}
function Add-Text($sh, $paras, $runs) {
  $text = ($paras | ForEach-Object { $_.Text }) -join ''
  $pfAll = [pscustomobject]@{ Bullet = [pscustomobject]@{ Visible = -1 } }
  $tr = [pscustomobject]@{ Text = $text; ParagraphFormat = $pfAll }
  Add-Indexer $tr 'Paragraphs' $paras
  Add-Indexer $tr 'Runs' $runs
  $tf = [pscustomobject]@{ HasText = -1; TextRange = $tr; Orientation = 1; VerticalAnchor = 3; MarginLeft = 7.2; MarginRight = 7.2; MarginTop = 3.6; MarginBottom = 3.6; WordWrap = -1 }
  $f2 = [pscustomobject]@{ Fill = [pscustomobject]@{ Visible = -1; Transparency = 0 }; Line = [pscustomobject]@{ Visible = 0 }; Shadow = [pscustomobject]@{ Visible = 0 }; Glow = [pscustomobject]@{ Radius = 0 } }
  $sh.HasTextFrame = -1
  $sh | Add-Member -MemberType NoteProperty -Name TextFrame -Value $tf
  $sh | Add-Member -MemberType NoteProperty -Name TextFrame2 -Value ([pscustomobject]@{ TextRange = [pscustomobject]@{ Font = $f2 } })
  return $sh
}
function New-Slide($shapes, $hidden) {
  $s = [pscustomobject]@{ SlideShowTransition = [pscustomobject]@{ Hidden = $hidden }; Shapes = $shapes }
  $s | Add-Member -MemberType ScriptMethod -Name Export -Value { param($path, $fmt, $w, $h) [IO.File]::WriteAllText($path, "PNG $w x $h") }
  return $s
}

# 슬라이드 1
$title = Add-Text (New-Shape 17 48 30 864 80 0 0 $null) @((New-Para "주일 예배`r" 2 0)) @((New-Run '주일 예배' 44 -1 65535))      # 글만 → 글상자
$body = Add-Text (New-Shape 17 60 150 840 300 0 0 $null) @((New-Para "첫째 줄`r" 1 -1), (New-Para '둘째 줄' 1 -1)) @((New-Run '첫째 줄' 28 0 16777215), (New-Run '둘째 줄' 28 0 16777215))
$group = [pscustomobject]@{ Type = 6; Visible = -1; Rotation = 0; GroupItems = @($body) }                                            # 그룹 안 글 → 글상자
$footer = Add-Text (New-Shape 14 0 500 300 30 0 0 @{ PlaceholderFormat = [pscustomobject]@{ Type = 15; ContainedType = 1 } }) @((New-Para '○○교회' 1 0)) @((New-Run '○○교회' 12 0 0))  # 바닥글 → 그대로
$turned = Add-Text (New-Shape 17 700 400 200 50 0 0 @{ Rotation = 15 }) @((New-Para '기울인 글' 1 0)) @((New-Run '기울인 글' 20 0 0))   # 돌린 것 → 그대로
$pic = New-Shape 13 600 300 300 200 0 0 $null; Add-Export $pic 'ok'                                                                    # 그림 → 요소 그림
$rect = Add-Text (New-Shape 1 40 440 200 60 -1 0 $null) @((New-Para '버튼' 2 0)) @((New-Run '버튼' 24 -1 0)); Add-Export $rect 'ok'      # 글 든 도형 → 그림 + 글상자
$bad = Add-Text (New-Shape 1 300 440 200 60 -1 -1 $null) @((New-Para '안내' 2 0)) @((New-Run '안내' 24 0 0)); Add-Export $bad 'fail'      # 그림으로 못 꺼냄 → 글만
$empty = New-Shape 1 0 0 50 50 0 0 $null; Add-Export $empty 'ok'                                                                       # 보이는 것 없음 → 그대로
$s1 = New-Slide @($title, $group, $footer, $turned, $pic, $rect, $bad, $empty) 0
# 슬라이드 2: 숨긴 슬라이드 (내보내지 않음)
$s2 = New-Slide @((Add-Text (New-Shape 17 0 0 100 100 0 0 $null) @((New-Para '숨김' 1 0)) @((New-Run '숨김' 20 0 0)))) -1
# 슬라이드 3: 글상자 하나
$s3 = New-Slide @((Add-Text (New-Shape 17 100 200 760 140 0 0 $null) @((New-Para '광고' 3 0)) @((New-Run '광고' 54 0 255)))) 0

$pres = [pscustomobject]@{ PageSetup = [pscustomobject]@{ SlideWidth = 960; SlideHeight = 540 }; Slides = @($s1, $s2, $s3); Saved = 0; Closed = $false }
$pres | Add-Member -MemberType ScriptMethod -Name Close -Value { $this.Closed = $true }
$presentations = [pscustomobject]@{ Count = 0 }
$presentations | Add-Member -MemberType ScriptMethod -Name Open -Value { param($f, $ro, $untitled, $win) return $pres }
$global:MockApp = [pscustomobject]@{ Presentations = $presentations }
$global:MockApp | Add-Member -MemberType ScriptMethod -Name Quit -Value { }

$script = [IO.File]::ReadAllText($env:WC_PPT_SCRIPT).Replace('New-Object -ComObject PowerPoint.Application', '$global:MockApp')
$out = & ([scriptblock]::Create($script))
Write-Output $out
# 숨긴 요소 (Visible: 0 = 꺼내서 숨김) · 도형 안 글 숨김 (글자 채우기 Visible: 0 = 숨김) · 저장 안 함 표시 · 닫음
Write-Output ('VISIBLE ' + (@($title, $body, $footer, $turned, $pic, $rect, $bad, $empty) | ForEach-Object { $_.Visible }) -join ' ')
Write-Output ('TEXTFILL ' + (@($rect, $bad, $footer) | ForEach-Object { $_.TextFrame2.TextRange.Font.Fill.Visible }) -join ' ')
Write-Output ('SAVED ' + $pres.Saved + ' CLOSED ' + $pres.Closed)
