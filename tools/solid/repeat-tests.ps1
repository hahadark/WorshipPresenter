param([int]$Rounds = 3, [string]$Out = 'tmp/solid/repeat')
$ErrorActionPreference = 'Stop'
$root = Resolve-Path "$PSScriptRoot/../.."
$outDir = [IO.Path]::GetFullPath((Join-Path $root $Out))
New-Item -ItemType Directory -Force $outDir | Out-Null
$env:RUSTC_BOOTSTRAP = '1' # libtest shuffle only; this script does not compile
$env:WP_LEGACY_DATA = Join-Path $outDir 'empty-legacy'
New-Item -ItemType Directory -Force $env:WP_LEGACY_DATA | Out-Null
$summary = @()
foreach ($crate in @('app', 'core')) {
    $binary = Get-ChildItem "$root/target/debug/deps/wp_$crate-*.exe" | Sort-Object LastWriteTime -Descending | Select-Object -First 1
    if (!$binary) { throw "Build cargo test -p wp-$crate --no-run first" }
    for ($round = 1; $round -le $Rounds; $round++) {
        foreach ($threads in @(3, 12, 32)) {
            $seed = 617261100 + ($round * 97) + $threads
            $log = Join-Path $outDir "$crate-$round-$threads.log"
            Push-Location "$root/crates/wp-$crate"
            try {
                $job = Start-Process -FilePath $binary.FullName -ArgumentList @('-Z','unstable-options','--shuffle-seed', $seed, '--test-threads', $threads) -RedirectStandardOutput $log -RedirectStandardError "$log.err" -WindowStyle Hidden -PassThru
                $job.WaitForExit()
                $row = [ordered]@{ crate = $crate; round = $round; threads = $threads; seed = $seed; exit = $job.ExitCode; binary = $binary.Name; result = @(Select-String -Path $log -Pattern '^test result:').Line }
                $summary += $row
                $row | ConvertTo-Json -Compress | Write-Output
                $summary | ConvertTo-Json -Depth 5 | Set-Content -Encoding utf8 (Join-Path $outDir 'summary.json')
            } finally { Pop-Location }
        }
    }
}
if (@($summary | Where-Object exit -ne 0).Count) { exit 1 }
