# 워십프리젠터 Windows 릴리스 파일 만들기 (로컬)
#   tools\release.ps1 -Version 2.0.0
# 만드는 것 (dist\):
#   WorshipPresenter-<버전>-Setup.exe     설치 파일 (Electron 판 자리·바로가기·자동 업데이트를 이어받음)
#   WorshipPresenter-<버전>-Portable.zip  설치 없이 쓰는 묶음
#   latest.yml                            Electron 판 자동 업데이트(electron-updater)가 읽는 파일
# 올리기는 사람이 확인한 뒤 따로 (gh release create …). 이 스크립트는 아무것도 올리지 않는다.
param(
  [Parameter(Mandatory = $true)][string]$Version,
  [switch]$SkipBuild
)
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root
$env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"

# 버전 확인: Cargo.toml 과 같아야 한다 (앱이 자기 버전을 업데이트 비교에 씀)
$cargoVer = (Select-String crates\wp-core\Cargo.toml -Pattern '^version = "(.+)"').Matches[0].Groups[1].Value
if ($cargoVer -ne $Version) { throw "Cargo.toml 버전($cargoVer)과 -Version($Version)이 다릅니다." }

if (-not $SkipBuild) {
  cargo build --release -p wp-app -p wp-output --offline
  if ($LASTEXITCODE -ne 0) { throw '릴리스 빌드 실패' }
}

$dist = Join-Path $root 'dist'
$stage = Join-Path $dist 'stage'
if (Test-Path $stage) { Remove-Item $stage -Recurse -Force }
New-Item -ItemType Directory -Force $stage | Out-Null

Copy-Item target\release\wp-app.exe (Join-Path $stage 'WorshipPresenter.exe')
Copy-Item target\release\wp-output.exe $stage
Get-ChildItem target\release -Filter *.dll -ErrorAction SilentlyContinue | Copy-Item -Destination $stage
Copy-Item web (Join-Path $stage 'web') -Recurse
Copy-Item LICENSE (Join-Path $stage 'LICENSE.txt')
Copy-Item THIRD_PARTY_NOTICES.md $stage
Copy-Item assets\fonts\OFL-Pretendard.txt $stage
# 기본 성경(개역한글, 공개 자료)만. 저작권 있는 번역본은 넣지 않는다.
$seed = 'assets\bible-seed.db'
if (-not (Test-Path $seed)) { throw "$seed 가 없습니다 (Electron 판 data\bible.db 를 복사해 두세요)." }
Copy-Item $seed (Join-Path $stage 'bible.db')

# 아이콘: 여러 크기(16~256)가 든 assets\icon.ico (실행 파일에 넣는 것과 같은 파일)
Copy-Item assets\icon.ico (Join-Path $stage 'WorshipPresenter.ico')

# 설치 파일
$makensis = Get-ChildItem "$env:LOCALAPPDATA\electron-builder\Cache\nsis" -Recurse -Filter makensis.exe -ErrorAction SilentlyContinue |
  Where-Object { $_.DirectoryName -notmatch '\\Bin$' } | Select-Object -First 1 -ExpandProperty FullName
if (-not $makensis) { $makensis = (Get-Command makensis -ErrorAction SilentlyContinue).Source }
if (-not $makensis) { throw 'makensis 를 찾지 못했습니다 (NSIS 3).' }
$setupName = "WorshipPresenter-$Version-Setup.exe"
$setup = Join-Path $dist $setupName
& $makensis /V2 /INPUTCHARSET UTF8 "/DVERSION=$Version" "/DSRC=$stage" "/DOUT=$setup" installer\WorshipPresenter.nsi
if ($LASTEXITCODE -ne 0) { throw 'makensis 실패' }

# 포터블 묶음
$zip = Join-Path $dist "WorshipPresenter-$Version-Portable.zip"
if (Test-Path $zip) { Remove-Item $zip }
Compress-Archive -Path (Join-Path $stage '*') -DestinationPath $zip -CompressionLevel Optimal

# latest.yml (electron-updater 형식: sha512 는 base64)
$sha = [Convert]::ToBase64String([Security.Cryptography.SHA512]::Create().ComputeHash([IO.File]::ReadAllBytes($setup)))
$size = (Get-Item $setup).Length
$date = (Get-Date).ToUniversalTime().ToString("yyyy-MM-ddTHH:mm:ss.fffZ")
$yml = @"
version: $Version
files:
  - url: $setupName
    sha512: $sha
    size: $size
path: $setupName
sha512: $sha
releaseDate: '$date'
"@
[IO.File]::WriteAllText((Join-Path $dist 'latest.yml'), ($yml -replace "`r`n", "`n") + "`n", (New-Object Text.UTF8Encoding($false)))

Get-ChildItem $dist -File | Select-Object Name, @{ n = 'MB'; e = { [math]::Round($_.Length / 1MB, 1) } } | Format-Table -AutoSize
