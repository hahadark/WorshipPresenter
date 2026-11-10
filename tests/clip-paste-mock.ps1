# 클립보드의 슬라이드를 PowerPoint 에 붙여 넣어 저장하는 스크립트(clippaste.js PS_CLIP_PASTE)를 가짜 PowerPoint 객체로 돌려 본다 (tests/core.test.js 가 부름)
#   실제 PowerPoint 없이 스크립트의 흐름만 점검: 원본 서식 유지 → 안 되면 그냥 붙여 넣기 → .pptx 로 저장 → 우리가 만든 것만 닫기 · 사용자가 열어 둔 것이 있으면 PowerPoint 는 끄지 않기
#   $env:WC_PPT_SCRIPT = 점검할 스크립트 파일, $env:WC_PPT_FILE = 저장할 파일
#   $env:WC_MOCK_MODE = keep (원본 서식 유지가 됨) | plain (그 명령이 안 됨) | nowindow (창을 못 만듦) | none (클립보드에 슬라이드가 없음)
$ErrorActionPreference = 'Stop'
$mode = $env:WC_MOCK_MODE
$log = New-Object System.Collections.ArrayList

$slides = [pscustomobject]@{ Count = 0 }
$slides | Add-Member -MemberType ScriptMethod -Name Paste -Value {
  if ($mode -eq 'none') { throw 'Slides.Paste : Invalid request. Clipboard is empty or contains data which may not be pasted here.' }
  $this.Count = 2
  [void]$log.Add('Paste')
}
$win = [pscustomobject]@{ ViewType = 9 }
$win | Add-Member -MemberType ScriptMethod -Name Activate -Value { [void]$log.Add('Activate') }
$windows = [pscustomobject]@{ Count = 1 }
$windows | Add-Member -MemberType ScriptMethod -Name Item -Value { param($i) if ($mode -eq 'nowindow') { throw 'Windows.Item : Integer out of range.' }; return $win }
$pres = [pscustomobject]@{ Slides = $slides; Windows = $windows; Saved = 0; Closed = $false }
$pres | Add-Member -MemberType ScriptMethod -Name SaveAs -Value { param($f, $fmt) [IO.File]::WriteAllText($f, "PPTX $fmt $($this.Slides.Count)"); [void]$log.Add("SaveAs$fmt") }
$pres | Add-Member -MemberType ScriptMethod -Name Close -Value { $this.Closed = $true }
# 사용자가 열어 둔 프레젠테이션이 하나 있다 → PowerPoint 를 끄면 안 된다
$presentations = [pscustomobject]@{ Count = 1 }
$presentations | Add-Member -MemberType ScriptMethod -Name Add -Value {
  param($withWindow)
  if ($mode -eq 'nowindow' -and $withWindow -ne 0) { throw 'Presentations.Add : cannot create a window.' }
  [void]$log.Add("Add$withWindow")
  return $pres
}
$bars = [pscustomobject]@{ Name = 'CommandBars' }
$bars | Add-Member -MemberType ScriptMethod -Name ExecuteMso -Value {
  param($id)
  [void]$log.Add("Mso:$id")
  if ($mode -eq 'keep') { $slides.Count = 3 } else { throw 'ExecuteMso : the command is not available.' }
}
$global:MockApp = [pscustomobject]@{ Presentations = $presentations; CommandBars = $bars; Stopped = $false }
$global:MockApp | Add-Member -MemberType ScriptMethod -Name Quit -Value { $this.Stopped = $true }

$script = [IO.File]::ReadAllText($env:WC_PPT_SCRIPT).Replace('New-Object -ComObject PowerPoint.Application', '$global:MockApp')
try {
  $out = & ([scriptblock]::Create($script))
  Write-Output $out
} catch {
  Write-Output ('ERR ' + $_.Exception.Message)
}
Write-Output ('LOG ' + ($log -join ' '))
Write-Output ('VIEW ' + $win.ViewType + ' SAVED ' + $pres.Saved + ' CLOSED ' + $pres.Closed + ' QUIT ' + $global:MockApp.Stopped)
