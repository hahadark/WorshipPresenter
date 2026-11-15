# 설치 파일이 남의 파일을 지우지 않는지 시험한다 (임시 폴더에서만, /S /TEST 로만).
#   tools\install-guard-test.ps1                  지금 installer\WorshipPresenter.nsi 를 시험 (하나라도 틀리면 종료 코드 1)
#   tools\install-guard-test.ps1 -Baseline <커밋>  그 커밋의 설치 스크립트로 같은 시험 — 고치기 전 위험이 실제로 나는지 보기 (틀린 것을 나열만 함)
#   -Keep  시험 폴더를 지우지 않고 남긴다
# 안전: 실제 프로그램을 쓰지 않는다. 바로 끝나는 가짜 실행 파일과 작은 가짜 자료로 설치 파일을 따로 만들고,
#       %TEMP%\wp-install-guard-* 안의 폴더에만 /S /TEST /D= 로 설치·제거한다. 레지스트리·바로가기·실제 설치는 건드리지 않는다.
param(
  [string]$Nsi,
  [string]$Baseline,
  [switch]$Keep
)
$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
if (-not $Nsi) { $Nsi = Join-Path $repo 'installer\WorshipPresenter.nsi' }

$makensis = Get-ChildItem "$env:LOCALAPPDATA\electron-builder\Cache\nsis" -Recurse -Filter makensis.exe -ErrorAction SilentlyContinue |
  Where-Object { $_.DirectoryName -notmatch '\\Bin$' } | Select-Object -First 1 -ExpandProperty FullName
if (-not $makensis) { $makensis = (Get-Command makensis -ErrorAction SilentlyContinue).Source }
if (-not $makensis) { throw 'makensis 를 찾지 못했습니다 (NSIS 3).' }

# 경로에 공백과 한글을 일부러 넣는다
$root = Join-Path ([IO.Path]::GetTempPath()) ("wp-install-guard-" + [Guid]::NewGuid().ToString('N').Substring(0, 8))
$work = Join-Path $root '시험 폴더'
New-Item -ItemType Directory -Force $work | Out-Null
$script:fails = New-Object System.Collections.Generic.List[string]
$script:passes = 0
$script:case = ''

function Put([string]$path, [string]$text) {
  New-Item -ItemType Directory -Force (Split-Path $path -Parent) | Out-Null
  [IO.File]::WriteAllText($path, $text)
}
function Snap([string]$dir) {
  $h = @{}
  if (Test-Path -LiteralPath $dir) {
    Get-ChildItem -LiteralPath $dir -Recurse -File -Force | ForEach-Object {
      $h[$_.FullName.Substring($dir.Length + 1)] = (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash
    }
  }
  $h
}
function Ok([bool]$cond, [string]$what) {
  if ($cond) { $script:passes++; Write-Host "    통과  $what" }
  else { $script:fails.Add("[$script:case] $what"); Write-Host "    틀림  $what" -ForegroundColor Red }
}
# 앞에서 찍어 둔 파일이 모두 그대로인가
function Kept($before, [string]$dir, [string]$what) {
  $now = Snap $dir
  $lost = @($before.Keys | Where-Object { -not $now.ContainsKey($_) } | Sort-Object)
  $changed = @($before.Keys | Where-Object { $now.ContainsKey($_) -and $now[$_] -ne $before[$_] } | Sort-Object)
  $detail = ''
  if ($lost.Count) { $detail += " — 사라짐: " + ($lost -join ', ') }
  if ($changed.Count) { $detail += " — 바뀜: " + ($changed -join ', ') }
  Ok (($lost.Count + $changed.Count) -eq 0) "$what ($($before.Count)개 파일 그대로)$detail"
}
function InRoot([string]$dir) {
  if (-not $dir.StartsWith($root + '\', [StringComparison]::OrdinalIgnoreCase)) { throw "시험 폴더 밖입니다: $dir" }
}
function WaitUnlocked([string]$dir) {
  # 설치 끝에 한 번 켜지는 가짜 실행 파일이 끝날 때까지
  foreach ($n in 'WorshipPresenter.exe', 'wp-output.exe') {
    $f = Join-Path $dir $n
    for ($i = 0; $i -lt 50 -and (Test-Path -LiteralPath $f); $i++) {
      try { $s = [IO.File]::Open($f, 'Open', 'ReadWrite', 'None'); $s.Close(); break } catch { Start-Sleep -Milliseconds 200 }
    }
  }
}
# /S /TEST /D=<폴더> 로 설치. 종료 코드를 돌려준다. $realDirs = 실제로 설치됐을 수 있는 폴더들
function Install([string]$dir, [string[]]$realDirs) {
  InRoot $dir
  $p = Start-Process -FilePath $script:setup -ArgumentList "/S /TEST /D=$dir" -PassThru -Wait
  foreach ($d in @($dir.TrimEnd('\')) + $realDirs) { WaitUnlocked $d }
  $p.ExitCode
}
function Uninstall([string]$dir) {
  InRoot $dir
  $un = Join-Path $dir 'Uninstall WorshipPresenter.exe'
  if (-not (Test-Path -LiteralPath $un)) { return $false }
  Start-Process -FilePath $un -ArgumentList '/S' -Wait
  # 제거 파일은 자신을 임시 폴더로 옮겨 다시 실행하므로, 원래 파일이 없어질 때까지 기다린다
  for ($i = 0; $i -lt 100 -and (Test-Path -LiteralPath $un); $i++) { Start-Sleep -Milliseconds 200 }
  Start-Sleep -Milliseconds 500
  -not (Test-Path -LiteralPath $un)
}
function HasOurs([string]$dir) {
  (Test-Path -LiteralPath (Join-Path $dir 'WorshipPresenter.exe')) -and (Test-Path -LiteralPath (Join-Path $dir 'web\output\index.html'))
}
# 다른 Electron 프로그램 폴더 (워십프리젠터가 아님) + 소중한 파일들
function MakeForeignApp([string]$dir) {
  Put "$dir\OtherApp.exe" 'other app'
  Put "$dir\icudtl.dat" 'icu'
  Put "$dir\LICENSES.chromium.html" 'lic'
  Put "$dir\LICENSE.electron.txt" 'lic'
  Put "$dir\vk_swiftshader_icd.json" '{}'
  Put "$dir\chrome_100_percent.pak" 'pak'
  Put "$dir\resources.pak" 'pak'
  Put "$dir\ffmpeg.dll" 'dll'
  Put "$dir\otherapp-core.dll" 'dll'
  Put "$dir\snapshot_blob.bin" 'bin'
  Put "$dir\precious-data.bin" 'bin'
  Put "$dir\locales\ko.pak" 'pak'
  Put "$dir\resources\app.asar" 'other asar'
  Put "$dir\resources\user-database.sqlite" 'db'
  Put "$dir\web\output\index.html" '<html>other app</html>'
  Put "$dir\web\my-site\photo.txt" 'photo'
  Put "$dir\bible.db" 'not ours'
  Put "$dir\LICENSE.txt" 'not ours'
}

try {
  # ── 가짜 스테이징과 설치 파일 ──
  $stage = Join-Path $root 'stage'
  New-Item -ItemType Directory -Force $stage | Out-Null
  $stub = Join-Path $root 'stub.exe'
  $csc = Join-Path $env:WINDIR 'Microsoft.NET\Framework64\v4.0.30319\csc.exe'
  if (Test-Path $csc) {
    # 창 없이 바로 끝나는 실행 파일
    $cs = Join-Path $root 'stub.cs'
    Put $cs 'static class P { static int Main() { return 0; } }'
    & $csc /nologo /target:winexe "/out:$stub" $cs | Out-Null
  }
  if (-not (Test-Path $stub)) { Copy-Item (Join-Path $env:WINDIR 'System32\where.exe') $stub }
  Copy-Item $stub (Join-Path $stage 'WorshipPresenter.exe')
  Copy-Item $stub (Join-Path $stage 'wp-output.exe')
  Copy-Item (Join-Path $repo 'assets\icon.ico') (Join-Path $stage 'WorshipPresenter.ico')
  Copy-Item (Join-Path $repo 'web') (Join-Path $stage 'web') -Recurse
  foreach ($n in 'bible.db', 'LICENSE.txt', 'THIRD_PARTY_NOTICES.md', 'OFL-Pretendard.txt', 'wp-fake-runtime.dll') { Put (Join-Path $stage $n) "가짜 $n" }
  $stageFiles = @(Get-ChildItem -LiteralPath $stage -Recurse -File | ForEach-Object { $_.FullName.Substring($stage.Length + 1) })

  $nsiDir = Split-Path $Nsi -Parent
  if ($Baseline) {
    $nsiDir = Join-Path $root 'baseline'
    New-Item -ItemType Directory -Force $nsiDir | Out-Null
    $Nsi = Join-Path $nsiDir 'WorshipPresenter.nsi'
    Push-Location $repo
    try {
      cmd /c "git show ${Baseline}:installer/WorshipPresenter.nsi > `"$Nsi`""
      if ($LASTEXITCODE -ne 0) { throw "git show $Baseline 실패" }
      cmd /c "git show ${Baseline}:installer/filelist.ps1 > `"$nsiDir\filelist.ps1`" 2>nul"
    } finally { Pop-Location }
  }
  $script:setup = Join-Path $root 'Setup.exe'
  $log = & $makensis /V2 /INPUTCHARSET UTF8 '/DVERSION=0.0.0' "/DSRC=$stage" "/DOUT=$script:setup" $Nsi 2>&1
  if ($LASTEXITCODE -ne 0 -or -not (Test-Path $script:setup)) { $log | Write-Host; throw 'makensis 실패' }
  Write-Host ("설치 파일 만듦: " + $(if ($Baseline) { "고치기 전($Baseline)" } else { '지금 스크립트' }) + " · 시험 폴더 $root")

  # ── 1. 남의 Electron 프로그램 폴더에 /S /TEST /D= ──
  $script:case = '1 남의 Electron 폴더'; Write-Host "`n$script:case"
  $d = Join-Path $work 'OtherApp'
  MakeForeignApp $d
  $before = Snap $d
  $code = Install $d @("$d\WorshipPresenter")
  Kept $before $d '남의 파일이 모두 남음'
  Ok (-not (Test-Path "$d\WorshipPresenter.exe")) '남의 폴더에 그대로 풀지 않음'
  Ok ((HasOurs "$d\WorshipPresenter") -or $code -ne 0) "안쪽 WorshipPresenter 폴더에 설치되었거나 실패로 끝남 (종료 코드 $code)"
  if (HasOurs "$d\WorshipPresenter") {
    Ok (Uninstall "$d\WorshipPresenter") '안쪽 폴더 제거가 끝남'
    Ok (-not (Test-Path "$d\WorshipPresenter")) '제거 뒤 안쪽 폴더가 없어짐'
    Kept $before $d '제거 뒤에도 남의 파일이 모두 남음'
  } elseif (Test-Path "$d\Uninstall WorshipPresenter.exe") {
    # 고치기 전: 남의 폴더에 그대로 풀렸다 → 그 제거 파일이 무엇을 더 지우는지
    [void](Uninstall $d)
    Kept $before $d '제거 뒤에도 남의 파일이 모두 남음'
  }

  # ── 2. 남의 폴더이고 그 안의 WorshipPresenter 폴더도 남의 것 → 아무것도 하지 않고 실패 ──
  $script:case = '2 안쪽 폴더까지 남의 것'; Write-Host "`n$script:case"
  $d = Join-Path $work 'OtherApp2'
  MakeForeignApp $d
  Put "$d\WorshipPresenter\someone-else.dll" 'dll'
  Put "$d\WorshipPresenter\web\output\index.html" '<html>theirs</html>'
  $before = Snap $d
  $code = Install $d @("$d\WorshipPresenter", "$d\WorshipPresenter\WorshipPresenter")
  Kept $before $d '남의 파일이 모두 남음'
  Ok ($code -eq 3) "종료 코드 3 으로 끝남 (실제 $code)"
  Ok ((Snap $d).Count -eq $before.Count) '새로 생긴 파일이 없음'

  # ── 3. 없는 폴더에 새로 설치 → 제거 ──
  $script:case = '3 새 폴더 설치·제거'; Write-Host "`n$script:case"
  $d = Join-Path $work '새 설치'
  $code = Install $d @()
  Ok ($code -eq 0) "설치 종료 코드 0 (실제 $code)"
  $missing = @($stageFiles | Where-Object { -not (Test-Path -LiteralPath (Join-Path $d $_)) })
  Ok ($missing.Count -eq 0) ("넣을 파일 $($stageFiles.Count)개가 모두 있음" + $(if ($missing.Count) { ' — 없음: ' + ($missing -join ', ') }))
  Ok (-not (Test-Path "$d\WorshipPresenter")) '안쪽 폴더를 따로 만들지 않음'
  if (-not $Baseline) {
    $listed = @(Get-Content -LiteralPath "$d\wp-files.txt" -ErrorAction SilentlyContinue | Where-Object { $_ -like 'F *' } | ForEach-Object { $_.Substring(2) })
    Ok ((@(Compare-Object ($stageFiles | Sort-Object) ($listed | Sort-Object)).Count) -eq 0) "파일 목록(wp-files.txt)이 넣은 파일과 같음 ($($listed.Count)개)"
  }
  Ok (Uninstall $d) '제거가 끝남'
  Ok (-not (Test-Path -LiteralPath $d)) '제거 뒤 폴더가 통째로 없어짐'

  # ── 4. 다시 설치·제거: 사용자가 둔 DLL 과 web\ 안 파일 ──
  $script:case = '4 재설치·제거와 사용자 파일'; Write-Host "`n$script:case"
  $d = Join-Path $work 'Reinstall'
  $code = Install $d @()
  Put "$d\my-plugin.dll" 'user dll'
  Put "$d\notes.bin" 'user bin'
  Put "$d\web\my-notes.txt" 'user notes'
  Put "$d\web\output\custom.css" 'user css'
  Put "$d\web\내 자료\사진.txt" 'user'
  $user = @{}
  foreach ($k in 'my-plugin.dll', 'notes.bin', 'web\my-notes.txt', 'web\output\custom.css', 'web\내 자료\사진.txt') { $user[$k] = (Get-FileHash -LiteralPath "$d\$k" -Algorithm SHA256).Hash }
  if (-not $Baseline) {
    # 앞 판에는 있었고 이번 판에는 없는 파일 흉내: 목록에 적혀 있으면 지워져야 한다
    Put "$d\web\output\old-removed.js" 'old'
    Put "$d\web\old-dir\gone.js" 'old'
    Add-Content -LiteralPath "$d\wp-files.txt" -Value "F web\output\old-removed.js`r`nF web\old-dir\gone.js`r`nD web\old-dir" -Encoding ASCII
  }
  $code = Install $d @("$d\WorshipPresenter")
  Ok ($code -eq 0) "재설치 종료 코드 0 (실제 $code)"
  Ok ((HasOurs $d) -and -not (Test-Path "$d\WorshipPresenter")) '같은 폴더에 다시 설치됨 (안쪽 폴더로 빠지지 않음)'
  Kept $user $d '재설치 뒤 사용자 파일이 남음'
  if (-not $Baseline) {
    Ok (-not (Test-Path "$d\web\output\old-removed.js") -and -not (Test-Path "$d\web\old-dir")) '앞 판 목록에만 있던 옛 파일·폴더는 지워짐'
  }
  Ok (Uninstall $d) '제거가 끝남'
  Kept $user $d '제거 뒤 사용자 파일이 남음'
  $left = @((Snap $d).Keys | Where-Object { -not $user.ContainsKey($_) } | Sort-Object)
  Ok ($left.Count -eq 0) ('제거 뒤 우리 파일은 남지 않음' + $(if ($left.Count) { ' — 남음: ' + ($left -join ', ') }))

  # ── 5. 워십프리젠터 Electron 판 폴더(확인됨) → 그 자리에 설치, 아는 Electron 파일만 정리 ──
  $script:case = '5 워십프리젠터 Electron 판 폴더'; Write-Host "`n$script:case"
  $d = Join-Path $work 'WorshipCue'
  $electron = 'icudtl.dat', 'LICENSES.chromium.html', 'LICENSE.electron.txt', 'vk_swiftshader_icd.json', 'chrome_100_percent.pak', 'chrome_200_percent.pak',
    'resources.pak', 'snapshot_blob.bin', 'v8_context_snapshot.bin', 'd3dcompiler_47.dll', 'ffmpeg.dll', 'libEGL.dll', 'libGLESv2.dll', 'vk_swiftshader.dll', 'vulkan-1.dll',
    'locales\ko.pak', 'locales\en-US.pak', 'resources\app.asar', 'resources\app-update.yml', 'resources\elevate.exe', 'resources\bible.db', 'resources\app.asar.unpacked\node_modules\x\y.node'
  foreach ($f in $electron) { Put "$d\$f" 'electron' }
  Put "$d\WorshipPresenter.exe" 'electron exe'
  Put "$d\Uninstall WorshipPresenter.exe" 'electron uninstaller'
  Put "$d\my-plugin.dll" 'user dll'
  Put "$d\notes.bin" 'user bin'
  Put "$d\keep.pak" 'user pak'
  Put "$d\resources\user-note.txt" 'user'
  $user = @{}
  foreach ($k in 'my-plugin.dll', 'notes.bin', 'keep.pak', 'resources\user-note.txt') { $user[$k] = (Get-FileHash -LiteralPath "$d\$k" -Algorithm SHA256).Hash }
  $code = Install $d @("$d\WorshipPresenter")
  Ok ($code -eq 0) "설치 종료 코드 0 (실제 $code)"
  Ok ((HasOurs $d) -and -not (Test-Path "$d\WorshipPresenter") -and (Get-Item "$d\WorshipPresenter.exe").Length -eq (Get-Item $stub).Length) '그 자리에 설치되어 실행 파일이 새 것으로 바뀜'
  $leftE = @($electron | Where-Object { Test-Path -LiteralPath "$d\$_" })
  Ok ($leftE.Count -eq 0) ('Electron 판 파일이 정리됨' + $(if ($leftE.Count) { ' — 남음: ' + ($leftE -join ', ') }))
  Kept $user $d '사용자가 둔 dll·bin·pak·resources 안 파일이 남음'

  # ── 6. 파일 목록이 없는 Rust 판(2.0.x) 위에 설치 ──
  $script:case = '6 목록 없는 옛 Rust 판'; Write-Host "`n$script:case"
  $d = Join-Path $work 'OldRust'
  Put "$d\WorshipPresenter.exe" 'old rust exe'
  Put "$d\wp-output.exe" 'old'
  Put "$d\web\output\index.html" '<html>old</html>'
  Put "$d\web\user.txt" 'user'
  Put "$d\my-plugin.dll" 'user dll'
  $user = @{}
  foreach ($k in 'web\user.txt', 'my-plugin.dll') { $user[$k] = (Get-FileHash -LiteralPath "$d\$k" -Algorithm SHA256).Hash }
  $code = Install $d @("$d\WorshipPresenter")
  Ok ($code -eq 0) "설치 종료 코드 0 (실제 $code)"
  Ok ((HasOurs $d) -and -not (Test-Path "$d\WorshipPresenter") -and (Get-Item "$d\WorshipPresenter.exe").Length -eq (Get-Item $stub).Length) '그 자리에 설치되어 실행 파일이 새 것으로 바뀜'
  Kept $user $d '사용자가 둔 web\user.txt 와 dll 이 남음'

  if (-not $Baseline) {
    # ── 7. 파일 목록에 바깥 경로·별표가 적혀 있어도 설치 폴더 밖이나 묶음으로 지우지 않는다 ──
    $script:case = '7 조작된 파일 목록'; Write-Host "`n$script:case"
    $base = Join-Path $work 'Tamper'
    $d = Join-Path $base 'app'
    $code = Install $d @()
    Put "$base\outside.txt" 'outside'
    Put "$base\victim\keep.txt" 'outside'
    Put "$base\abs.txt" 'outside'
    Put "$d\my-plugin.dll" 'user dll'
    $before = Snap $base
    $ourKeys = @($stageFiles) + 'wp-files.txt', 'Uninstall WorshipPresenter.exe'
    foreach ($k in $ourKeys) { $before.Remove($k) ; $before.Remove("app\$k") }
    Add-Content -LiteralPath "$d\wp-files.txt" -Encoding ASCII -Value @(
      'F ..\outside.txt', 'F web\..\..\outside.txt', "F $base\abs.txt", 'F \abs.txt', 'F *.dll', 'F my-plugin.d?l',
      'D ..\victim', 'F ..\victim\keep.txt', 'X my-plugin.dll', 'F ', 'my-plugin.dll')
    $code = Install $d @("$d\WorshipPresenter")
    Ok ($code -eq 0 -and (HasOurs $d)) "재설치 종료 코드 0 (실제 $code)"
    Kept $before $base '바깥 파일과 사용자 dll 이 남음'

    # ── 8. 끝에 역슬래시를 붙인 /D= ──
    $script:case = '8 끝 역슬래시'; Write-Host "`n$script:case"
    $d = Join-Path $work 'OtherApp3'
    MakeForeignApp $d
    $before = Snap $d
    $code = Install "$d\" @("$d\WorshipPresenter")
    Kept $before $d '남의 파일이 모두 남음'
    Ok ((HasOurs "$d\WorshipPresenter") -and -not (Test-Path "$d\WorshipPresenter.exe")) '안쪽 WorshipPresenter 폴더에 설치됨'
  }

  Write-Host ''
  if ($Baseline) {
    Write-Host "고치기 전($Baseline) 결과: 통과 $script:passes · 틀림 $($script:fails.Count)"
    $script:fails | ForEach-Object { Write-Host "  재현됨  $_" -ForegroundColor Yellow }
    $exit = 0
  } else {
    Write-Host "결과: 통과 $script:passes · 틀림 $($script:fails.Count)"
    $script:fails | ForEach-Object { Write-Host "  틀림  $_" -ForegroundColor Red }
    $exit = if ($script:fails.Count) { 1 } else { 0 }
  }
} finally {
  if ($Keep) { Write-Host "시험 폴더를 남김: $root" }
  else {
    for ($i = 0; $i -lt 10 -and (Test-Path -LiteralPath $root); $i++) {
      Remove-Item -LiteralPath $root -Recurse -Force -ErrorAction SilentlyContinue
      if (Test-Path -LiteralPath $root) { Start-Sleep -Milliseconds 500 }
    }
  }
}
exit $exit
