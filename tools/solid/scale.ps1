# 배율 · 창 크기 캡처: 화면 배율(설정 ui.scale 100 · 125 · 150 · 200) × 창 크기(가장 작은 1000x640 · 1280x800 · 1600x960 · 1920x1080)로
# 주 화면(성경봉독 송출 중)을 찍고, 주요 창을 150% · 가장 작은 창에서 찍는다. 격리 자료 · 로고 없는 템플릿 · COMPUTERNAME=TEST-PC.
# 사용: powershell -File tools\solid\scale.ps1 [-Exe target\release\wp-app.exe] [-Out tmp\solid\scale]
param([string]$Exe = 'target\release\wp-app.exe', [string]$Out = 'tmp\solid\scale', [string]$Only = '')
$ErrorActionPreference = 'Stop'
$root = Resolve-Path "$PSScriptRoot\..\.."
Set-Location $root
$exe = (Resolve-Path $Exe).Path
$fix = Join-Path $root 'tmp\solid\fixtures'
if (!(Test-Path "$fix\seed\bible.db")) { & python tools\solid\fixtures.py --out $fix | Out-Null }
$outDir = Join-Path $root $Out
New-Item -ItemType Directory -Force $outDir | Out-Null
$env:WP_LEGACY_DATA = Join-Path $fix 'empty-legacy'
$env:COMPUTERNAME = 'TEST-PC'

function Shot([string]$name, [int]$scale, [string]$win, [string[]]$extra) {
    if ($Only -and $name -notlike "*$Only*") { return }
    $d = Join-Path $outDir "data"
    if (Test-Path $d) { Remove-Item -Recurse -Force $d }
    Copy-Item -Recurse "$fix\seed" $d
    $s = Get-Content "$d\settings.json" -Raw -Encoding UTF8 | ConvertFrom-Json
    $s.ui.scale = $scale
    $s | ConvertTo-Json -Depth 50 | Set-Content "$d\settings.json" -Encoding UTF8
    $png = Join-Path $outDir "$name.png"
    $a = @('--data', $d, '--import-cue', "$fix\cue.json", '--win', $win, '--search', 'x', '--item', '6', '--send', '--shot', $png) + $extra
    $p = Start-Process $exe -ArgumentList $a -PassThru
    if (!$p.WaitForExit(60000)) { Stop-Process -Id $p.Id -Force }
    foreach ($c in (Get-CimInstance Win32_Process | Where-Object { $_.ParentProcessId -eq $p.Id })) { Stop-Process -Id $c.ProcessId -Force -ErrorAction SilentlyContinue }
    "$name $(if (Test-Path $png) { 'ok' } else { 'MISSING' })"
}

foreach ($win in '1000x640', '1280x800', '1600x960', '1920x1080') {
    foreach ($scale in 100, 125, 150, 200) { Shot "main-$win-s$scale" $scale $win @() }
}
# 주요 창 (가장 작은 창 · 150%, 큰 창 · 200%)
$windows = [ordered]@{
    'settings' = @('--open-c', 'settings'); 'scene' = @('--scene-open', 'pip'); 'template' = @('--tpl-open', 'worship');
    'stage' = @('--open-menu', 'b:stage'); 'sync' = @('--open-menu', 'b:sync'); 'sources' = @('--ins-tab', 'sources')
}
foreach ($k in $windows.Keys) {
    Shot "win-$k-1000x640-s150" 150 '1000x640' $windows[$k]
    Shot "win-$k-1920x1080-s200" 200 '1920x1080' $windows[$k]
}
