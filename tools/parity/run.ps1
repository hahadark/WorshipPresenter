# 점검: 조작 화면 슬라이드 그림을 cases 마다 rust-<이름>.png 로 찍고, 웹 비교용 resolved.json · port.txt 를 남긴 뒤 close.txt 가 생길 때까지 서버를 띄워 둔다
param([string]$Cases, [string]$Out, [string]$Data = "$PSScriptRoot\..\..\tmp\data")
$ErrorActionPreference = 'Continue'
if (Test-Path $Out) { Remove-Item $Out -Recurse -Force }
$p = Start-Process $PSScriptRoot\..\..\target\debug\wp-app.exe -ArgumentList '--data',$Data,'--win','1280x720','--parity',$Cases,'--parity-out',$Out,'--parity-hold' -PassThru
$p.Id | Set-Content "$env:TEMP\wp-parity-pid.txt"
for ($i = 0; $i -lt 120; $i++) { if (Test-Path "$Out\done.txt") { break }; Start-Sleep 1 }
"PID=$($p.Id) done=$(Test-Path "$Out\done.txt") port=$(Get-Content "$Out\port.txt" -ErrorAction SilentlyContinue)"

