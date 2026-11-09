# PPT 요소 따로 고치기의 배경 그림 스크립트(pptrender.js PS_BG)를 가짜 PowerPoint 객체로 돌려 본다 (tests/core.test.js 가 부름)
#   실제 PowerPoint 없이 스크립트의 흐름만 점검: hide.json 의 도형 id(그룹 안 포함)만 숨기고 · 숨긴 슬라이드는 건너뛰고 · 슬라이드마다 PNG · 저장 안 함
#   $env:WC_PPT_SCRIPT = 점검할 스크립트 파일, $env:WC_PPT_OUT = 결과 폴더, $env:WC_PPT_HIDE = hide.json
$ErrorActionPreference = 'Stop'

function New-Shape($id, $type) {
  return [pscustomobject]@{ Id = $id; Type = $type; Visible = -1 }
}
function New-Slide($shapes, $hidden) {
  $s = [pscustomobject]@{ SlideShowTransition = [pscustomobject]@{ Hidden = $hidden }; Shapes = $shapes }
  $s | Add-Member -MemberType ScriptMethod -Name Export -Value { param($path, $fmt, $w, $h) [IO.File]::WriteAllText($path, "PNG $w x $h") }
  return $s
}

# 슬라이드 1: 글상자(2) · 그림(3) · 그룹 안 도형(5) — hide.json 은 2 와 5 를 숨긴다
$t = New-Shape 2 17
$pic = New-Shape 3 13
$inner = New-Shape 5 1
$group = [pscustomobject]@{ Id = 4; Type = 6; Visible = -1; GroupItems = @($inner) }
$s1 = New-Slide @($t, $pic, $group) 0
# 슬라이드 2: 숨긴 슬라이드 (내보내지 않음 · 번호도 매기지 않음)
$hid = New-Shape 7 17
$s2 = New-Slide @($hid) -1
# 슬라이드 3 (보이는 번호 2): 도형 9 를 숨긴다
$nine = New-Shape 9 1
$s3 = New-Slide @($nine) 0

$pres = [pscustomobject]@{ PageSetup = [pscustomobject]@{ SlideWidth = 960; SlideHeight = 540 }; Slides = @($s1, $s2, $s3); Saved = 0; Closed = $false }
$pres | Add-Member -MemberType ScriptMethod -Name Close -Value { $this.Closed = $true }
$presentations = [pscustomobject]@{ Count = 0 }
$presentations | Add-Member -MemberType ScriptMethod -Name Open -Value { param($f, $ro, $untitled, $win) return $pres }
$global:MockApp = [pscustomobject]@{ Presentations = $presentations }
$global:MockApp | Add-Member -MemberType ScriptMethod -Name Quit -Value { }

$script = [IO.File]::ReadAllText($env:WC_PPT_SCRIPT).Replace('New-Object -ComObject PowerPoint.Application', '$global:MockApp')
$out = & ([scriptblock]::Create($script))
Write-Output $out
# 숨긴 것(0)과 그대로 둔 것(-1)
Write-Output ('VISIBLE ' + (@($t, $pic, $inner, $hid, $nine) | ForEach-Object { $_.Visible }) -join ' ')
Write-Output ('SAVED ' + $pres.Saved + ' CLOSED ' + $pres.Closed)
