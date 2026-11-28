# [web] probe.js 가 띄운 브라우저만 끈다 (프로필 폴더가 tmp\web-*\profile-* 인 msedge, chrome)
Get-CimInstance Win32_Process | Where-Object { ($_.Name -eq 'msedge.exe' -or $_.Name -eq 'chrome.exe') -and $_.CommandLine -like '*\tmp\web-*\profile-*' } | ForEach-Object { Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }
