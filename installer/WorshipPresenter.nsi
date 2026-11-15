; 워십프리젠터 (Rust 판) Windows 설치 파일
; - Electron 판(1.x)과 같은 자리·같은 실행 파일 이름·같은 제거 항목을 써서 그대로 바꿔 끼운다.
;   (바탕화면·시작 메뉴 '워십프리젠터' 바로가기와 Electron 판 자동 업데이트(latest.yml)가 그대로 이어진다)
; - Electron 판 자동 업데이트는 이 파일을 "--updated /S" 로 조용히 실행한다. "--force-run" 이면 설치 뒤 다시 켠다.
; - 사용자 자료(%APPDATA%\WorshipCue, %APPDATA%\WorshipPresenter-rs)는 건드리지 않는다.
; - /TEST : 점검용. 레지스트리·바로가기를 쓰지 않는다 (/D=<임시 폴더> 와 함께).
; 빌드: tools\release.ps1 이 makensis /DVERSION=… /DSRC=… /DOUT=… 로 부른다.

Unicode true
SetCompressor /SOLID lzma
RequestExecutionLevel user
ManifestDPIAware true

!include "MUI2.nsh"
!include "FileFunc.nsh"
!include "LogicLib.nsh"

!define GUID "a2fe4d9a-c2b8-5a19-bbdf-4e2660eaf113" ; electron-builder appId org.worshipcue.app 에서 나온 값
!define UNKEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\${GUID}"
!define APPKEY "Software\${GUID}"
!define EXE "WorshipPresenter.exe"
!define UNEXE "Uninstall WorshipPresenter.exe"
!define SHORTCUT "워십프리젠터"

Name "워십프리젠터 ${VERSION}"
OutFile "${OUT}"
InstallDir "$LOCALAPPDATA\Programs\WorshipCue"
BrandingText "워십프리젠터 ${VERSION}"
VIProductVersion "${VERSION}.0"
VIAddVersionKey "ProductName" "WorshipPresenter"
VIAddVersionKey "FileDescription" "워십프리젠터 설치"
VIAddVersionKey "FileVersion" "${VERSION}"
VIAddVersionKey "ProductVersion" "${VERSION}"
VIAddVersionKey "LegalCopyright" "GPL-3.0-or-later"

Var Updated
Var ForceRun
Var TestMode

!define MUI_ICON "${SRC}\WorshipPresenter.ico"
!define MUI_UNICON "${SRC}\WorshipPresenter.ico"
!define MUI_ABORTWARNING
!define MUI_FINISHPAGE_RUN "$INSTDIR\${EXE}"
!define MUI_FINISHPAGE_RUN_TEXT "워십프리젠터 실행"
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "Korean"

Function .onInit
  StrCpy $Updated 0
  StrCpy $ForceRun 0
  StrCpy $TestMode 0
  ${GetParameters} $R0
  ClearErrors
  ${GetOptions} $R0 "--updated" $R1
  ${IfNot} ${Errors}
    StrCpy $Updated 1
  ${EndIf}
  ClearErrors
  ${GetOptions} $R0 "--force-run" $R1
  ${IfNot} ${Errors}
    StrCpy $ForceRun 1
  ${EndIf}
  ClearErrors
  ${GetOptions} $R0 "/TEST" $R1
  ${IfNot} ${Errors}
    StrCpy $TestMode 1
  ${EndIf}
  ; /D= 를 주지 않았으면 지금 설치된 자리(Electron 판 포함)를 그대로 쓴다
  ${If} $TestMode == 0
    ReadRegStr $0 HKCU "${APPKEY}" "InstallLocation"
    ${If} $0 != ""
    ${AndIf} ${FileExists} "$0\*.*"
      StrCpy $INSTDIR $0
    ${EndIf}
  ${EndIf}
FunctionEnd

; 켜져 있는 워십프리젠터(이전 판 포함)가 꺼질 때까지 기다린다. 강제로 끄지 않는다.
Function WaitForExit
  StrCpy $1 0
  retry:
    ClearErrors
    ${If} ${FileExists} "$INSTDIR\${EXE}"
      Delete "$INSTDIR\${EXE}"
    ${EndIf}
    ${If} ${FileExists} "$INSTDIR\wp-output.exe"
      Delete "$INSTDIR\wp-output.exe"
    ${EndIf}
    ${If} ${FileExists} "$INSTDIR\${EXE}"
    ${OrIf} ${FileExists} "$INSTDIR\wp-output.exe"
      IntOp $1 $1 + 1
      ${If} $1 < 60
        Sleep 500
        Goto retry
      ${EndIf}
      IfSilent 0 +2
        Abort "워십프리젠터가 아직 켜져 있어 설치하지 못했습니다."
      MessageBox MB_RETRYCANCEL|MB_ICONEXCLAMATION "워십프리젠터가 켜져 있습니다.$\r$\n프로그램을 끈 뒤 [다시 시도] 를 누르세요." IDRETRY again
      Abort "워십프리젠터가 켜져 있어 설치를 그만두었습니다."
      again:
        StrCpy $1 0
        Goto retry
    ${EndIf}
FunctionEnd

Section "워십프리젠터"
  SetShellVarContext current
  CreateDirectory "$INSTDIR"
  Call WaitForExit

  ; Electron 판 파일 정리 (Rust 판은 쓰지 않음)
  RMDir /r "$INSTDIR\locales"
  RMDir /r "$INSTDIR\resources"
  RMDir /r "$INSTDIR\web"
  Delete "$INSTDIR\*.pak"
  Delete "$INSTDIR\*.dll"
  Delete "$INSTDIR\*.bin"
  Delete "$INSTDIR\icudtl.dat"
  Delete "$INSTDIR\LICENSE.electron.txt"
  Delete "$INSTDIR\LICENSES.chromium.html"
  Delete "$INSTDIR\vk_swiftshader_icd.json"

  SetOutPath "$INSTDIR"
  File "${SRC}\${EXE}"
  File "${SRC}\wp-output.exe"
  File "${SRC}\WorshipPresenter.ico"
  File "${SRC}\bible.db"
  File "${SRC}\LICENSE.txt"
  File "${SRC}\THIRD_PARTY_NOTICES.md"
  File "${SRC}\OFL-Pretendard.txt"
  File /r "${SRC}\web"
  File /nonfatal /r "${SRC}\*.dll"

  WriteUninstaller "$INSTDIR\${UNEXE}"

  ${If} $TestMode == 0
    WriteRegStr HKCU "${APPKEY}" "InstallLocation" "$INSTDIR"
    WriteRegStr HKCU "${APPKEY}" "ShortcutName" "${SHORTCUT}"
    WriteRegStr HKCU "${UNKEY}" "DisplayName" "WorshipPresenter ${VERSION}"
    WriteRegStr HKCU "${UNKEY}" "DisplayVersion" "${VERSION}"
    WriteRegStr HKCU "${UNKEY}" "Publisher" "WorshipPresenter"
    WriteRegStr HKCU "${UNKEY}" "DisplayIcon" "$INSTDIR\WorshipPresenter.ico"
    WriteRegStr HKCU "${UNKEY}" "InstallLocation" "$INSTDIR"
    WriteRegStr HKCU "${UNKEY}" "UninstallString" '"$INSTDIR\${UNEXE}" /currentuser'
    WriteRegStr HKCU "${UNKEY}" "QuietUninstallString" '"$INSTDIR\${UNEXE}" /currentuser /S'
    WriteRegDWORD HKCU "${UNKEY}" "NoModify" 1
    WriteRegDWORD HKCU "${UNKEY}" "NoRepair" 1
    ${GetSize} "$INSTDIR" "/S=0K" $0 $1 $2
    WriteRegDWORD HKCU "${UNKEY}" "EstimatedSize" $0

    ; 바로가기: 같은 이름으로 덮어써 아이콘까지 새로 (위치·이름은 Electron 판과 같음)
    CreateShortcut "$DESKTOP\${SHORTCUT}.lnk" "$INSTDIR\${EXE}" "" "$INSTDIR\WorshipPresenter.ico" 0
    CreateShortcut "$SMPROGRAMS\${SHORTCUT}.lnk" "$INSTDIR\${EXE}" "" "$INSTDIR\WorshipPresenter.ico" 0
  ${EndIf}

  ${If} $ForceRun == 1
    Exec '"$INSTDIR\${EXE}"'
  ${EndIf}
SectionEnd

Section "Uninstall"
  SetShellVarContext current
  Delete "$INSTDIR\${EXE}"
  Delete "$INSTDIR\wp-output.exe"
  Delete "$INSTDIR\WorshipPresenter.ico"
  Delete "$INSTDIR\bible.db"
  Delete "$INSTDIR\LICENSE.txt"
  Delete "$INSTDIR\THIRD_PARTY_NOTICES.md"
  Delete "$INSTDIR\OFL-Pretendard.txt"
  Delete "$INSTDIR\*.dll"
  RMDir /r "$INSTDIR\web"
  Delete "$INSTDIR\${UNEXE}"
  RMDir "$INSTDIR"
  Delete "$DESKTOP\${SHORTCUT}.lnk"
  Delete "$SMPROGRAMS\${SHORTCUT}.lnk"
  DeleteRegKey HKCU "${UNKEY}"
  DeleteRegKey HKCU "${APPKEY}"
  ; 예배 자료(%APPDATA%\WorshipPresenter-rs, %APPDATA%\WorshipCue)는 지우지 않는다
SectionEnd
