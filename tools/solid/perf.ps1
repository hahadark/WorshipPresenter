# 성능 재기 (release 판): 격리 자료로 조작 화면을 띄워 상태마다 CPU · 초당 다시 그리기 · 메모리 · 시작 시간을 잰다.
# 실제 설치본 · 사용자 자료는 쓰지 않는다 (tools/solid/fixtures.py 로 만든 자료 사본). 자기가 띄운 PID 만 끈다.
# 사용: powershell -File tools\solid\perf.ps1 [-Exe target\release\wp-app.exe] [-Out tmp\solid\perf] [-Seconds 20] [-States idle,live,timer,video,output,psalm] [-Large]
param(
    [string]$Exe = 'target\release\wp-app.exe',
    [string]$Out = 'tmp\solid\perf',
    [int]$Seconds = 20,
    [string]$States = 'idle,live,timer,video,output',
    [switch]$Large
)
$ErrorActionPreference = 'Stop'
$StateList = @($States -split '[,\s]+' | ForEach-Object { $_.Trim() } | Where-Object { $_ })
$root = Resolve-Path "$PSScriptRoot\..\.."
Set-Location $root
$exe = (Resolve-Path $Exe).Path
$outExe = Join-Path (Split-Path $exe) 'wp-output.exe'
$fix = Join-Path $root ($(if ($Large) { 'tmp\solid\fixtures-large' } else { 'tmp\solid\fixtures' }))
if (!(Test-Path "$fix\seed\bible.db")) {
    $a = @('tools\solid\fixtures.py', '--out', $fix); if ($Large) { $a += '--large' }
    & python @a | Out-Null
}
$outDir = Join-Path $root $Out
New-Item -ItemType Directory -Force $outDir | Out-Null
$env:WP_LEGACY_DATA = Join-Path $fix 'empty-legacy'
$env:WP_DEBUG_REPAINT = '1'
$cores = [Environment]::ProcessorCount
$results = @()

function Get-Tree([int]$id) {
    # 그 프로세스와 자식(WebView2 의 msedgewebview2 들)
    $all = Get-CimInstance Win32_Process
    $ids = @($id); $grew = $true
    while ($grew) {
        $grew = $false
        foreach ($p in $all) { if ($ids -contains $p.ParentProcessId -and -not ($ids -contains $p.ProcessId)) { $ids += $p.ProcessId; $grew = $true } }
    }
    $ids
}
function Get-Cpu([int[]]$ids) {
    $t = 0.0; $ws = 0; $pm = 0
    foreach ($i in $ids) { $p = Get-Process -Id $i -ErrorAction SilentlyContinue; if ($p) { $t += $p.TotalProcessorTime.TotalSeconds; $ws += $p.WorkingSet64; $pm += $p.PrivateMemorySize64 } }
    @{ cpu = $t; ws = $ws; pm = $pm }
}
function Get-Port([int]$id) {
    for ($k = 0; $k -lt 40; $k++) {
        $c = Get-NetTCPConnection -OwningProcess $id -State Listen -ErrorAction SilentlyContinue | Where-Object { $_.LocalAddress -eq '127.0.0.1' } | Sort-Object LocalPort | Select-Object -First 1
        if ($c) { return $c.LocalPort }
        Start-Sleep -Milliseconds 250
    }
    0
}

foreach ($state in $StateList) {
    $data = Join-Path $outDir "data-$state"
    if (Test-Path $data) { Remove-Item -Recurse -Force $data }
    Copy-Item -Recurse "$fix\seed" $data
    $item = switch ($state) { 'video' { 8 } 'psalm' { 9 } 'idle' { $null } default { 6 } }
    $args2 = @('--data', $data, '--import-cue', "$fix\cue.json", '--win', '1600x960')
    if ($null -ne $item) { $args2 += @('--item', "$item") }
    # 점검 인자(--search 가 있어야 --import-cue · --item · --send 가 돈다). 2.1.1 까지는 점검 모드가 끝나지 않고 매 프레임 다시 그렸으므로
    # 2.1.1 과 견줄 때는 idle(점검 인자 없음)만 같은 조건이다
    if ($state -ne 'idle') { $args2 += @('--search', 'x', '--send') }
    $t0 = Get-Date
    $p = Start-Process $exe -ArgumentList $args2 -PassThru
    $log = Join-Path $data "logs\$((Get-Date).ToString('yyyy-MM-dd')).log"
    $startup = ''
    for ($k = 0; $k -lt 120; $k++) {
        if (Test-Path $log) { $line = Select-String -Path $log -Pattern '첫 프레임 끝' -SimpleMatch | Select-Object -Last 1; if ($line) { $startup = $line.Line; break } }
        Start-Sleep -Milliseconds 250
    }
    $readyMs = [int]((Get-Date) - $t0).TotalMilliseconds
    $port = Get-Port $p.Id
    Start-Sleep 8   # 점검 인자(가져오기 · 미리보기 · 송출) 끝나기 기다림
    $op = $null
    if ($state -eq 'timer' -and $port) { Invoke-WebRequest -UseBasicParsing "http://127.0.0.1:$port/api/trigger/timer?min=30&key=solid-local-tests" | Out-Null }
    if ($state -in @('output', 'video') -and $port) {
        $op = Start-Process $outExe -ArgumentList @('--url', "http://127.0.0.1:$port/outputs/main?window=1", '--monitor', '0', '--windowed') -PassThru
        Start-Sleep 6
    }
    $appIds = Get-Tree $p.Id
    $a0 = Get-Cpu $appIds; $o0 = if ($op) { Get-Cpu (Get-Tree $op.Id) } else { $null }
    $mark = (Get-Content $log -Encoding UTF8).Count
    $w0 = Get-Date
    Start-Sleep $Seconds
    $dt = ((Get-Date) - $w0).TotalSeconds
    $a1 = Get-Cpu $appIds; $o1 = if ($op) { Get-Cpu (Get-Tree $op.Id) } else { $null }
    $lines = Get-Content $log -Encoding UTF8 | Select-Object -Skip $mark
    $frames = 0; $avg = @(); $max = 0.0
    foreach ($l in $lines) {
        if ($l -match '다시 그리기 2초 (\d+)번, 프레임 평균 ([\d.]+)ms · 최대 ([\d.]+)ms') { $frames += [int]$Matches[1]; $avg += [double]$Matches[2]; $max = [math]::Max($max, [double]$Matches[3]) }
    }
    $causes = ($lines | Where-Object { $_ -match '부른 자리' } | Select-Object -Last 1) -replace '^.*부른 자리: ', ''
    $row = [ordered]@{
        state = $state; seconds = [math]::Round($dt, 1)
        app_cpu_pct = [math]::Round(100 * ($a1.cpu - $a0.cpu) / $dt / $cores, 2)
        app_cpu_core_pct = [math]::Round(100 * ($a1.cpu - $a0.cpu) / $dt, 1)
        fps = [math]::Round($frames / $dt, 2)
        frame_avg_ms = $(if ($avg.Count) { [math]::Round(($avg | Measure-Object -Average).Average, 2) } else { 0 })
        frame_max_ms = $max
        app_ws_mb = [math]::Round($a1.ws / 1MB, 0); app_private_mb = [math]::Round($a1.pm / 1MB, 0)
        output_cpu_core_pct = $(if ($op) { [math]::Round(100 * ($o1.cpu - $o0.cpu) / $dt, 1) } else { $null })
        output_ws_mb = $(if ($op) { [math]::Round($o1.ws / 1MB, 0) } else { $null })
        ready_ms = $readyMs; startup = $startup -replace '^.*?\] ', ''; repaint_causes = $causes
    }
    $results += [pscustomobject]$row
    $row | ConvertTo-Json -Compress | Write-Output
    if ($op) { Stop-Process -Id $op.Id -Force -ErrorAction SilentlyContinue }
    Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue
    Start-Sleep 2
}
$name = if ($Large) { 'perf-large.json' } else { 'perf.json' }
$results | ConvertTo-Json -Depth 4 | Set-Content -Encoding utf8 (Join-Path $outDir $name)
