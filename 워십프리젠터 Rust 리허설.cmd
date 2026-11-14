@echo off
chcp 65001 >nul
rem 리허설: 기존 워십프리젠터 자료를 tmp\rehearsal-data 로 복사해 씁니다 (원본은 바꾸지 않음)
start "" "%~dp0target\debug\wp-app.exe" --data "%~dp0tmp\rehearsal-data"
