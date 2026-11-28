# '새 버전' 받기 → SHA-256 검증 → 설치 → 새 판으로 켜기를 끝까지 (임시 설치본 · 실제 설치본은 읽기만 해서 전후 비교)
#  1 공개 2.1.1 설치 파일을 받아 임시 폴더에 /S /TEST /D= 로 설치 (레지스트리 · 바로가기 안 건드림)
#  2 그 폴더의 WorshipPresenter.exe 를 이 작업트리의 release 판으로 바꿔 '2.1.0 인 척'(WP_UPDATE_AS_VERSION) 켬
#    WP_UPDATE_E2E=1: 켜자마자 새 버전 확인 → 받기 · 검증 → 설치 실행(WP_UPDATE_TEST_DIR = 그 임시 폴더라 /TEST /D=) → 스스로 꺼짐
#  3 설치 파일이 끝나면 임시 폴더의 실행 파일이 공개 2.1.1 과 같은지, --data 임시 자료로 켜지는지 확인
#  4 실제 설치본(%LOCALAPPDATA%\Programs\WorshipCue) 실행 파일 해시 · 레지스트리 설치 자리가 그대로인지
# 사용: powershell -File tools\solid\update-e2e.ps1 [-Tag v2.1.1] [-Out tmp\solid\upd]
param([string]$Tag = 'v2.1.1', [string]$Out = 'tmp\solid\upd', [string]$AsVersion = '2.1.0')
$ErrorActionPreference = 'Stop'
$root = Resolve-Path "$PSScriptRoot\..\.."
Set-Location $root
$outDir = Join-Path $root $Out
if (Test-Path $outDir) { Remove-Item -Recurse -Force $outDir }
New-Item -ItemType Directory -Force $outDir | Out-Null
$fix = Join-Path $root 'tmp\solid\fixtures'
if (!(Test-Path "$fix\seed\bible.db")) { & python tools\solid\fixtures.py --out $fix | Out-Null }
$env:WP_LEGACY_DATA = Join-Path $fix 'empty-legacy'

function Get-RealGuard {
    $exe = Join-Path $env:LOCALAPPDATA 'Programs\WorshipCue\WorshipPresenter.exe'
    $h = if (Test-Path $exe) { (Get-FileHash $exe -Algorithm SHA256).Hash } else { '(없음)' }
    $g = 'a2fe4d9a-c2b8-5a19-bbdf-4e2660eaf113'
    $reg = Get-ItemProperty "HKCU:\Software\$g" -ErrorAction SilentlyContinue
    $un = Get-ItemProperty "HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\$g" -ErrorAction SilentlyContinue
    "exe=$h reg=$($reg.InstallLocation) uninstall=$($un.DisplayVersion)"
}
$before = Get-RealGuard
"실제 설치본(전): $before"

# 1 설치 파일 받기 · 임시 설치
$dl = Join-Path $outDir 'dl'
& gh release download $Tag -R hahadark/WorshipPresenter -p '*Setup.exe' -D $dl | Out-Null
$setup = Get-ChildItem "$dl\*Setup.exe" | Select-Object -First 1
$inst = Join-Path $outDir 'installed'
$p = Start-Process $setup.FullName -ArgumentList @('/S', '/TEST', "/D=$inst") -PassThru; $p.WaitForExit()
"임시 설치 ($($setup.Name)): 종료 코드 $($p.ExitCode), 실행 파일 $(Test-Path "$inst\WorshipPresenter.exe")"
$shipped = (Get-FileHash "$inst\WorshipPresenter.exe" -Algorithm SHA256).Hash

# 2 이 작업트리 판으로 바꿔 옛 판인 척 켬
Copy-Item -Force target\release\wp-app.exe "$inst\WorshipPresenter.exe"
$data = Join-Path $outDir 'data'; Copy-Item -Recurse "$fix\seed" $data
$env:WP_UPDATE_E2E = '1'; $env:WP_UPDATE_AS_VERSION = $AsVersion; $env:WP_UPDATE_TEST_DIR = $inst
$t0 = Get-Date
$app = Start-Process "$inst\WorshipPresenter.exe" -ArgumentList @('--data', $data, '--win', '1280x800') -PassThru
if (!$app.WaitForExit(600000)) { Stop-Process -Id $app.Id -Force; throw '10분 안에 끝나지 않음' }
Remove-Item Env:WP_UPDATE_E2E, Env:WP_UPDATE_AS_VERSION, Env:WP_UPDATE_TEST_DIR
$log = Get-Content (Get-ChildItem "$data\logs\*.log" | Select-Object -First 1).FullName -Encoding UTF8
$log | Select-String '업데이트 점검' | ForEach-Object { '  ' + ($_.Line -replace '^\S+ \S+ ', '').Substring(0, [math]::Min(200, ($_.Line -replace '^\S+ \S+ ', '').Length)) }

# 3 설치 파일이 끝날 때까지 (실행 파일이 공개 판으로 바뀜)
$done = $false
for ($k = 0; $k -lt 120; $k++) {
    $h = try { (Get-FileHash "$inst\WorshipPresenter.exe" -Algorithm SHA256 -ErrorAction Stop).Hash } catch { '' }   # 설치 파일이 쓰는 중이면 못 읽음
    if ($h -eq $shipped -and !(Get-Process | Where-Object { $_.Path -like '*\update-cache\*' })) { $done = $true; break }
    Start-Sleep 1
}
"설치 끝: $done ($([int]((Get-Date) - $t0).TotalSeconds)초) — 임시 설치본 실행 파일이 공개 $Tag 와 같음"
$data2 = Join-Path $outDir 'data2'; Copy-Item -Recurse "$fix\seed" $data2
$shot = Join-Path $outDir 'after-update.png'
$r = Start-Process "$inst\WorshipPresenter.exe" -ArgumentList @('--data', $data2, '--win', '1280x800', '--search', 'x', '--shot', $shot) -PassThru
$r.WaitForExit(90000) | Out-Null
"새 판으로 켜짐: 캡처 $(Test-Path $shot)"

# 4 실제 설치본
$after = Get-RealGuard
"실제 설치본(후): $after"
"실제 설치본 그대로: $($before -eq $after)"
