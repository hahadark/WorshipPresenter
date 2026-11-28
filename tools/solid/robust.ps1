# 죽지 않기 점검: 실제로 일을 일으켜 본다 (격리 자료 · 자기가 띄운 PID 만 끈다)
#  1 panic-ui      조작 화면 프레임 안 오류 → 프로그램 · 예배당 창 계속, 로그에 남음
#  2 panic-relock  코어 잠금 재진입 → 멈추지 않고 오류로 (같은 결과)
#  3 output-kill   예배당 창(wp-output)을 밖에서 끊음 → 자동으로 다시 열림 (1분에 세 번까지, 네 번째는 알림만)
#  4 port-busy     설정 포트를 다른 프로그램이 쓰는 중 → 다음 포트로 켜짐
#  5 settings-bad  settings.json 이 깨짐 → 켜지고, 깨진 파일은 따로 보존
#  6 kill-save     설정 · 큐시트를 바꾸는 중 강제 종료 → 다시 켜도 DB 무결성 · 설정 JSON 그대로
# 사용: powershell -File tools\solid\robust.ps1 [-Exe target\debug\wp-app.exe] [-Out tmp\solid\robust] [-Cases 'panic-ui,output-kill']
param([string]$Exe = 'target\debug\wp-app.exe', [string]$Out = 'tmp\solid\robust', [string]$Cases = 'panic-ui,panic-relock,output-kill,port-busy,settings-bad,kill-save')
$ErrorActionPreference = 'Stop'
$root = Resolve-Path "$PSScriptRoot\..\.."
Set-Location $root
$exe = (Resolve-Path $Exe).Path
$fix = Join-Path $root 'tmp\solid\fixtures'
if (!(Test-Path "$fix\seed\bible.db")) { & python tools\solid\fixtures.py --out $fix | Out-Null }
$outDir = Join-Path $root $Out
New-Item -ItemType Directory -Force $outDir | Out-Null
$env:WP_LEGACY_DATA = Join-Path $fix 'empty-legacy'
$caseList = @($Cases -split '[,\s]+' | Where-Object { $_ })
$report = @()

function New-Data([string]$name) {
    $d = Join-Path $outDir "data-$name"
    if (Test-Path $d) { Remove-Item -Recurse -Force $d }
    Copy-Item -Recurse "$fix\seed" $d
    $d
}
function Start-App([string]$data, [string[]]$extra) {
    $a = @('--data', $data, '--import-cue', "$fix\cue.json", '--win', '1280x800', '--search', 'x', '--item', '6', '--send') + $extra
    Start-Process $exe -ArgumentList $a -PassThru
}
function Get-Log([string]$data) { $f = Get-ChildItem "$data\logs\*.log" -ErrorAction SilentlyContinue | Select-Object -First 1; if ($f) { Get-Content $f.FullName -Encoding UTF8 } else { @() } }
function Get-Outputs([int]$appId) { , @(Get-CimInstance Win32_Process -Filter "Name='wp-output.exe'" | Where-Object { $_.ParentProcessId -eq $appId }) }
function Stop-Tree([int]$id) {
    foreach ($c in (Get-CimInstance Win32_Process | Where-Object { $_.ParentProcessId -eq $id })) { Stop-Process -Id $c.ProcessId -Force -ErrorAction SilentlyContinue }
    Stop-Process -Id $id -Force -ErrorAction SilentlyContinue
    Start-Sleep 3   # 다음 점검이 같은 포트 · 창을 다투지 않게
}
function Add-Row([string]$case, [bool]$ok, [string]$note) {
    $r = [pscustomobject]@{ case = $case; ok = $ok; note = $note }
    $script:report += $r
    "$case`t$(if ($ok) { 'OK' } else { 'FAIL' })`t$note" | Write-Output
}

foreach ($case in $caseList) {
    switch ($case) {
        { $_ -in 'panic-ui', 'panic-relock' } {
            $d = New-Data $case
            $kind = $case.Substring(6)
            $p = Start-App $d @('--outwin-open', '--panic-test', $kind)
            Start-Sleep 35   # 켜는 데 걸리는 시간(바쁜 PC 에서 10초 넘음) + 8초 뒤 오류 + 지켜보기
            $alive = !$p.HasExited
            $outs = Get-Outputs $p.Id
            $log = Get-Log $d
            $logged = [bool]($log | Select-String -Pattern '일부러 낸 오류|코어 잠금 재진입')
            Add-Row $case ($alive -and $outs.Count -ge 1 -and $logged) "조작 화면 살아 있음=$alive, 예배당 창=$($outs.Count), 로그=$logged"
            Stop-Tree $p.Id
        }
        'output-kill' {
            $d = New-Data $case
            $p = Start-App $d @('--outwin-open')
            Start-Sleep 25
            $seen = @()
            for ($k = 1; $k -le 4; $k++) {
                $o = Get-Outputs $p.Id
                if (!$o.Count) { $seen += "[$k] 창 없음"; break }
                Stop-Process -Id $o[0].ProcessId -Force
                Start-Sleep 6
                $n = Get-Outputs $p.Id
                $seen += "[$k] 끊음 $($o[0].ProcessId) → $(if ($n.Count) { "다시 열림 $($n[0].ProcessId)" } else { '안 열림' })"
            }
            $log = Get-Log $d
            $again = @($log | Select-String -SimpleMatch '자동 다시 열기').Count
            # 세 번까지는 다시 열리고, 1분 안의 네 번째는 열지 않고 알림
            $ok = ($seen[0] -like '*다시 열림*') -and ($seen[1] -like '*다시 열림*') -and ($seen[2] -like '*다시 열림*') -and ($seen[3] -like '*안 열림*')
            Add-Row $case $ok (($seen -join ' / ') + " · 로그 '자동 다시 열기' $again")
            Stop-Tree $p.Id
        }
        'port-busy' {
            $d = New-Data $case
            $s = Get-Content "$d\settings.json" -Raw -Encoding UTF8 | ConvertFrom-Json
            $want = [int]$s.port
            $l = [System.Net.Sockets.TcpListener]::new([System.Net.IPAddress]::Loopback, $want); $l.Start()
            try {
                $p = Start-App $d @()
                Start-Sleep 12
                $ports = @(Get-NetTCPConnection -OwningProcess $p.Id -State Listen -ErrorAction SilentlyContinue | Where-Object LocalAddress -eq '127.0.0.1' | ForEach-Object LocalPort)
                $ok = !$p.HasExited -and ($ports -contains ($want + 1))
                Add-Row $case $ok "포트 $want 를 막아 둠 → 조작 화면이 연 포트: $($ports -join ',')"
                Stop-Tree $p.Id
            } finally { $l.Stop() }
        }
        'settings-bad' {
            $d = New-Data $case
            Set-Content "$d\settings.json" '{"port": 37990, "templates": [ {"id":' -Encoding UTF8
            $p = Start-App $d @()
            Start-Sleep 12
            $kept = @(Get-ChildItem "$d\settings.broken-*.json" -ErrorAction SilentlyContinue)
            $log = Get-Log $d
            $ok = !$p.HasExited -and $kept.Count -eq 1
            Add-Row $case $ok "켜짐=$(!$p.HasExited), 보존한 깨진 파일=$($kept.Count), 로그='$(($log | Select-String -SimpleMatch '깨진 설정' | Select-Object -First 1).Line)'"
            Stop-Tree $p.Id
        }
        'kill-save' {
            $d = New-Data $case
            $bad = 0; $runs = 5
            for ($k = 0; $k -lt $runs; $k++) {
                $p = Start-App $d @()
                Start-Sleep (8 + $k)   # 가져오기 · 송출 · 설정 저장이 도는 사이 아무 때나
                Stop-Process -Id $p.Id -Force
                Start-Sleep 1
                $ok1 = $true
                try { Get-Content "$d\settings.json" -Raw -Encoding UTF8 | ConvertFrom-Json | Out-Null } catch { $ok1 = $false }
                $ic = & python -c "import sqlite3,sys; print(sqlite3.connect(sys.argv[1]).execute('PRAGMA integrity_check').fetchone()[0])" "$d\bible.db"
                $tmp = @(Get-ChildItem $d -Filter '*.tmp' -ErrorAction SilentlyContinue).Count
                if (!$ok1 -or $ic -ne 'ok') { $bad++ }
                "  kill-save[$k] settings=$ok1 db=$ic 남은 임시 파일=$tmp" | Write-Output
            }
            Add-Row $case ($bad -eq 0) "$runs 번 강제 종료 · 깨짐 $bad"
        }
    }
}
$report | ConvertTo-Json | Set-Content -Encoding utf8 (Join-Path $outDir 'robust.json')
