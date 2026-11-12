@echo off
chcp 65001 >nul
if not exist "%~dp0target\debug\wp-app.exe" (
  echo 개발 실행 파일이 없습니다. 먼저 cargo build로 검증용 빌드를 만들어 주세요.
  echo 모든 기능 구현 전까지 출시 빌드와 릴리스는 중지되어 있습니다.
  pause
  exit /b 1
)
start "" "%~dp0target\debug\wp-app.exe" %*
