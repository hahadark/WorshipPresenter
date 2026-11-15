; 워십프리젠터 (Rust 판) Windows 설치 파일
; - Electron 판(1.x)과 같은 자리·같은 실행 파일 이름·같은 제거 항목을 써서 그대로 바꿔 끼운다.
;   (바탕화면·시작 메뉴 '워십프리젠터' 바로가기와 Electron 판 자동 업데이트(latest.yml)가 그대로 이어진다)
; - Electron 판 자동 업데이트는 이 파일을 "--updated /S" 로 조용히 실행한다. "--force-run" 이면 설치 뒤 다시 켠다.
; - Electron 판을 '모든 사용자'용(Program Files, HKLM)으로 설치한 PC: 그 자리를 이어받으려면 관리자 권한이 필요하므로
;   자신을 관리자 권한으로 다시 실행한다(UAC). 안 그러면 옛 판이 그대로 남고 새 판이 따로 설치되어 아이콘이 둘이 된다.
; - 사용자 자료(%APPDATA%\WorshipCue, %APPDATA%\WorshipPresenter-rs)는 건드리지 않는다.
; - /TEST : 점검용. 레지스트리·바로가기·업데이트 캐시를 건드리지 않는다 (/D=<임시 폴더> 와 함께).
; - 남의 폴더 보호: 설치할 폴더에 다른 파일이 있고 워십프리젠터 설치로 확인되지 않으면 그 안의 WorshipPresenter 폴더에
;   설치한다. 그 폴더도 남의 것이면 아무것도 하지 않고 끝낸다(종료 코드 3). 화면 설치·/S 조용한 설치 모두 같다.
; - 지우는 것은 '우리가 넣은 파일'뿐이다: 넣은 파일 목록을 설치 폴더의 wp-files.txt 에 적어 두고, 다시 설치·제거할 때
;   그 목록의 파일만 지운다. *.dll 이나 web\ 통째 같은 묶음 삭제는 하지 않는다 (사용자가 둔 파일이 남는다).
; - 시험: tools\install-guard-test.ps1 (임시 폴더에서 /S /TEST 로만 돈다).
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
!define MANIFEST "wp-files.txt" ; 이 설치 파일이 넣은 파일 목록 (다음 설치가 읽어 옛 파일만 골라 지운다)

; 넣을 파일 목록: 컴파일할 때 스테이징 폴더(${SRC})를 훑어 만든다 → WP_EACH_FILE / WP_EACH_DIR
; (makensis 는 이 .nsi 가 있는 폴더로 옮겨 와서 컴파일하므로 filelist.ps1 을 이름만으로 부른다. ${__FILEDIR__} 는 NSIS 3.0.4 에서
;  .nsi 를 상대 경로로 주면 폴더 이름이 겹쳐 나와 쓰지 않는다. makensis /NOCD 로는 컴파일하지 않는다)
!tempfile WP_FILELIST
!system 'powershell -NoProfile -ExecutionPolicy Bypass -File "filelist.ps1" -Src "${SRC}" -Out "${WP_FILELIST}"' = 0
!include "${WP_FILELIST}"
!delfile "${WP_FILELIST}"

Name "워십프리젠터 ${VERSION}"
OutFile "${OUT}"
InstallDir "" ; 비워 둔다: .onInit 에서 /D= 를 주었는지($INSTDIR 가 비어 있지 않음) 가려내려고
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
Var PerMachine    ; 1 = '모든 사용자'용으로 설치돼 있던 자리(HKLM)를 이어받는다
Var ElevatedChild ; 1 = 관리자 권한으로 다시 실행된 쪽
Var RegView64     ; 1 = HKLM 항목이 64비트 보기에 있다
Var ExplicitDir   ; 1 = /D=<폴더> 로 설치할 자리를 직접 주었다
Var DirKind       ; 설치할 폴더: 0 = 없거나 빔, 1 = 워십프리젠터 설치로 확인됨, 2 = 다른 파일이 있음, 3 = 안쪽 폴더까지 남의 것(설치 안 함)

!define MUI_ICON "${SRC}\WorshipPresenter.ico"
!define MUI_UNICON "${SRC}\WorshipPresenter.ico"
!define MUI_ABORTWARNING
!define MUI_FINISHPAGE_RUN
!define MUI_FINISHPAGE_RUN_TEXT "워십프리젠터 실행"
!define MUI_FINISHPAGE_RUN_FUNCTION RunApp
!define MUI_PAGE_CUSTOMFUNCTION_LEAVE DirLeave
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "Korean"

; HKLM 의 설치 자리 찾기 → $0 (없으면 ""), $RegView64
!macro FindMachineInstall
  SetRegView 64
  StrCpy $RegView64 1
  ReadRegStr $0 HKLM "${APPKEY}" "InstallLocation"
  ${If} $0 == ""
    SetRegView 32
    StrCpy $RegView64 0
    ReadRegStr $0 HKLM "${APPKEY}" "InstallLocation"
  ${EndIf}
  SetRegView 32
!macroend

Function .onInit
  StrCpy $Updated 0
  StrCpy $ForceRun 0
  StrCpy $TestMode 0
  StrCpy $PerMachine 0
  StrCpy $ElevatedChild 0
  StrCpy $RegView64 0
  StrCpy $ExplicitDir 1
  ${If} $INSTDIR == ""
    StrCpy $ExplicitDir 0
    StrCpy $INSTDIR "$LOCALAPPDATA\Programs\WorshipCue"
  ${EndIf}
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
  ${GetOptions} $R0 "--elevated" $R1
  ${IfNot} ${Errors}
    StrCpy $ElevatedChild 1
  ${EndIf}
  ClearErrors
  ${GetOptions} $R0 "/TEST" $R1
  ${IfNot} ${Errors}
    StrCpy $TestMode 1
  ${EndIf}
  ; /D= 를 주지 않았으면 지금 설치된 자리(Electron 판 포함)를 그대로 쓴다.
  ; (전에는 /D= 를 주어도 등록된 자리가 있으면 그쪽에 설치해 버렸다)
  ; 관리자 권한으로 다시 실행된 쪽은 다른 계정일 수 있으므로 HKCU 를 보지 않고 모든 사용자용 자리만 본다
  ${If} $TestMode == 0
  ${AndIf} $ExplicitDir == 0
    StrCpy $0 ""
    ${If} $ElevatedChild == 0
      ReadRegStr $0 HKCU "${APPKEY}" "InstallLocation"
    ${EndIf}
    ${If} $0 != ""
    ${AndIf} ${FileExists} "$0\*.*"
      StrCpy $INSTDIR $0
    ${Else}
      !insertmacro FindMachineInstall
      ${If} $0 != ""
      ${AndIf} ${FileExists} "$0\${EXE}"
        StrCpy $PerMachine 1
        StrCpy $INSTDIR $0
      ${EndIf}
    ${EndIf}
  ${EndIf}
  ${If} $PerMachine == 1
    UserInfo::GetAccountType
    Pop $1
    ${If} $1 != "Admin"
      ${If} $ElevatedChild == 1
        ; 권한을 얻지 못했다
        SetErrorLevel 5
        Quit
      ${EndIf}
      StrCpy $2 "--elevated"
      ${If} $Updated == 1
        StrCpy $2 "$2 --updated"
      ${EndIf}
      ${If} ${Silent}
        StrCpy $2 "$2 /S"
      ${EndIf}
      ClearErrors
      ExecShellWait "runas" "$EXEPATH" "$2"
      ${If} ${Errors}
        ${IfNot} ${Silent}
          MessageBox MB_OK|MB_ICONEXCLAMATION "워십프리젠터가 이 PC 의 모든 사용자용으로 설치되어 있어($INSTDIR) 새 판으로 바꾸려면 관리자 권한이 필요합니다.$\r$\n설치 파일을 다시 실행해 권한 요청에서 [예] 를 눌러 주세요."
        ${EndIf}
        SetErrorLevel 5
        Quit
      ${EndIf}
      ; 다시 켜기는 권한이 없는 이쪽에서 (관리자 권한으로 프로그램이 켜지지 않게)
      ${If} $ForceRun == 1
      ${AndIf} ${FileExists} "$INSTDIR\${EXE}"
        Exec '"$INSTDIR\${EXE}"'
      ${EndIf}
      Quit
    ${EndIf}
  ${EndIf}
FunctionEnd

; $DirKind ← $INSTDIR 가 어떤 폴더인가
; 워십프리젠터 설치로 '확인됨' = 실행 파일이 있고, 거기에 더해 우리 파일 목록(이 설치 파일이 적음)이나 wp-output.exe(Rust 판)나
; resources\app.asar(Electron 판)가 있다. icudtl.dat 같은 Electron 공통 파일만으로는 우리 것으로 보지 않는다.
Function ClassifyDir
  StrCpy $DirKind 0
  ${If} ${FileExists} "$INSTDIR\${EXE}"
    ${If} ${FileExists} "$INSTDIR\${MANIFEST}"
    ${OrIf} ${FileExists} "$INSTDIR\wp-output.exe"
    ${OrIf} ${FileExists} "$INSTDIR\resources\app.asar"
      StrCpy $DirKind 1
      Return
    ${EndIf}
  ${EndIf}
  ${If} ${FileExists} "$INSTDIR\*.*"
    FindFirst $0 $1 "$INSTDIR\*.*"
    cd_loop:
      StrCmp $1 "" cd_done
      StrCmp $1 "." cd_next
      StrCmp $1 ".." cd_next
      StrCpy $DirKind 2
      Goto cd_done
    cd_next:
      ClearErrors
      FindNext $0 $1
      IfErrors cd_done
      Goto cd_loop
    cd_done:
    FindClose $0
  ${EndIf}
FunctionEnd

; 설치할 폴더 정하기: 다른 파일이 들어 있는 폴더면 그 안에 WorshipPresenter 폴더를 만들어 설치한다
; (그 폴더에 그대로 풀면 남의 파일과 섞이고, 정리·제거할 때 남의 파일을 건드릴 수 있다).
; 안쪽 WorshipPresenter 폴더도 남의 것이면 $DirKind = 3 — 부른 쪽이 설치를 그만둔다.
; 폴더 고르기 화면을 떠날 때와 파일을 풀기 직전(/S 는 화면이 없으므로 여기서만 걸린다) 두 번 부른다. 두 번 불러도 결과가 같다.
Function GuardInstDir
  StrCpy $1 $INSTDIR "" -1
  ${If} $1 == "\"
    StrCpy $INSTDIR $INSTDIR -1
  ${EndIf}
  Call ClassifyDir
  ${If} $DirKind == 2
    StrCpy $INSTDIR "$INSTDIR\WorshipPresenter"
    Call ClassifyDir
    ${If} $DirKind == 2
      StrCpy $DirKind 3
    ${EndIf}
  ${EndIf}
FunctionEnd

Function DirLeave
  StrCpy $R8 $INSTDIR
  Call GuardInstDir
  ${If} $DirKind == 3
    MessageBox MB_OK|MB_ICONEXCLAMATION "이 폴더에는 다른 프로그램의 파일이 들어 있어 설치할 수 없습니다.$\r$\n$INSTDIR$\r$\n비어 있는 폴더를 골라 주세요."
    StrCpy $INSTDIR $R8
    Abort
  ${EndIf}
FunctionEnd

; 앞 판이 넣은 파일을 그 판의 파일 목록대로 지운다 (없어진 파일이 남지 않게). 목록에 없는 파일은 그대로 둔다.
; 목록의 길은 설치 폴더 안쪽 상대 경로만 받는다: 드라이브(:)·위로 가기(..)·별표·물음표·앞 역슬래시가 있으면 건너뛴다.
Function RemoveListedFiles
  ${IfNot} ${FileExists} "$INSTDIR\${MANIFEST}"
    Return
  ${EndIf}
  ClearErrors
  FileOpen $R0 "$INSTDIR\${MANIFEST}" r
  IfErrors rl_end
  rl_line:
    ClearErrors
    FileRead $R0 $R1
    IfErrors rl_close
  rl_trim:
    StrCpy $R2 $R1 1 -1
    StrCmp $R2 "$\r" rl_cut
    StrCmp $R2 "$\n" rl_cut
    Goto rl_parse
  rl_cut:
    StrCpy $R1 $R1 -1
    Goto rl_trim
  rl_parse:
    StrCpy $R2 $R1 2    ; "F " 파일, "D " 폴더
    StrCpy $R3 $R1 "" 2 ; 상대 경로
    StrCmp $R3 "" rl_line
    StrCpy $R4 $R3 1
    StrCmp $R4 "\" rl_line
    StrCmp $R4 " " rl_line
    StrCpy $R5 0
  rl_char:
    StrCpy $R4 $R3 1 $R5
    StrCmp $R4 "" rl_ok
    StrCmp $R4 ":" rl_line
    StrCmp $R4 "/" rl_line
    StrCmp $R4 "*" rl_line
    StrCmp $R4 "?" rl_line
    StrCmp $R4 '"' rl_line
    StrCmp $R4 "<" rl_line
    StrCmp $R4 ">" rl_line
    StrCmp $R4 "|" rl_line
    StrCpy $R6 $R3 2 $R5
    StrCmp $R6 ".." rl_line
    IntOp $R5 $R5 + 1
    Goto rl_char
  rl_ok:
    ${If} $R2 == "F "
      Delete "$INSTDIR\$R3"
    ${ElseIf} $R2 == "D "
      RMDir "$INSTDIR\$R3" ; 비었을 때만 지워진다
    ${EndIf}
    Goto rl_line
  rl_close:
    FileClose $R0
  rl_end:
    ClearErrors
FunctionEnd

; 마침 화면의 [실행]: 탐색기를 거쳐 켠다 (관리자 권한으로 설치했어도 프로그램은 보통 권한으로 켜지게)
Function RunApp
  Exec '"$WINDIR\explorer.exe" "$INSTDIR\${EXE}"'
FunctionEnd

; $3 ← 0/1 : 그 파일이 실행 중이라 잠겨 있나 (지우지 않고 쓰기 열기로 확인)
!macro CheckLocked FILE
  ${If} ${FileExists} "${FILE}"
    ClearErrors
    FileOpen $0 "${FILE}" a
    ${If} ${Errors}
      StrCpy $3 1
    ${Else}
      FileClose $0
    ${EndIf}
  ${EndIf}
!macroend

; 켜져 있는 워십프리젠터(이전 판 포함)가 꺼질 때까지 기다린다. 강제로 끄지 않는다.
; 파일을 지워 보는 대신 열어 본다: 하나만 지워진 채 그만두면 설치가 망가진다.
Function WaitForExit
  StrCpy $1 0
  retry:
    StrCpy $3 0
    !insertmacro CheckLocked "$INSTDIR\${EXE}"
    !insertmacro CheckLocked "$INSTDIR\wp-output.exe"
    ${If} $3 == 1
      IntOp $1 $1 + 1
      ${If} $1 < 60
        Sleep 500
        Goto retry
      ${EndIf}
      ${If} ${Silent}
        SetErrorLevel 2
        Abort "워십프리젠터가 아직 켜져 있어 설치하지 못했습니다."
      ${EndIf}
      MessageBox MB_RETRYCANCEL|MB_ICONEXCLAMATION "워십프리젠터가 켜져 있습니다.$\r$\n프로그램을 끈 뒤 [다시 시도] 를 누르세요." IDRETRY again
      Abort "워십프리젠터가 켜져 있어 설치를 그만두었습니다."
      again:
        StrCpy $1 0
        Goto retry
    ${EndIf}
FunctionEnd

!macro WriteInstallReg ROOT UNARG
  WriteRegStr ${ROOT} "${APPKEY}" "InstallLocation" "$INSTDIR"
  WriteRegStr ${ROOT} "${APPKEY}" "ShortcutName" "${SHORTCUT}"
  WriteRegStr ${ROOT} "${UNKEY}" "DisplayName" "WorshipPresenter ${VERSION}"
  WriteRegStr ${ROOT} "${UNKEY}" "DisplayVersion" "${VERSION}"
  WriteRegStr ${ROOT} "${UNKEY}" "Publisher" "WorshipPresenter"
  WriteRegStr ${ROOT} "${UNKEY}" "DisplayIcon" "$INSTDIR\WorshipPresenter.ico"
  WriteRegStr ${ROOT} "${UNKEY}" "InstallLocation" "$INSTDIR"
  WriteRegStr ${ROOT} "${UNKEY}" "UninstallString" '"$INSTDIR\${UNEXE}" ${UNARG}'
  WriteRegStr ${ROOT} "${UNKEY}" "QuietUninstallString" '"$INSTDIR\${UNEXE}" ${UNARG} /S'
  WriteRegDWORD ${ROOT} "${UNKEY}" "NoModify" 1
  WriteRegDWORD ${ROOT} "${UNKEY}" "NoRepair" 1
  ${GetSize} "$INSTDIR" "/S=0K" $0 $1 $2
  WriteRegDWORD ${ROOT} "${UNKEY}" "EstimatedSize" $0
!macroend

!macro WP_INSTALL_FILE SUBDIR REL
  SetOutPath "$INSTDIR${SUBDIR}"
  File "${SRC}\${REL}"
!macroend
!macro WP_LIST_FILE SUBDIR REL
  FileWrite $R0 "F ${REL}$\r$\n"
!macroend
!macro WP_LIST_DIR REL
  FileWrite $R0 "D ${REL}$\r$\n"
!macroend
!macro WP_DELETE_FILE SUBDIR REL
  Delete "$INSTDIR\${REL}"
!macroend
!macro WP_REMOVE_DIR REL
  RMDir "$INSTDIR\${REL}" ; 비었을 때만 지워진다 (사용자가 둔 파일이 있으면 남는다)
!macroend

Section "워십프리젠터"
  ${If} $PerMachine == 1
    SetShellVarContext all
  ${Else}
    SetShellVarContext current
  ${EndIf}
  ; 남의 폴더 보호 (/S 는 폴더 고르기 화면이 없어 DirLeave 를 거치지 않는다 → 반드시 여기서 한 번 더 본다)
  Call GuardInstDir
  ${If} $DirKind == 3
    ${IfNot} ${Silent}
      MessageBox MB_OK|MB_ICONEXCLAMATION "이 폴더에는 다른 프로그램의 파일이 들어 있어 설치할 수 없습니다.$\r$\n$INSTDIR"
    ${EndIf}
    SetErrorLevel 3
    Abort "다른 프로그램의 파일이 들어 있는 폴더라 설치하지 않았습니다: $INSTDIR"
  ${EndIf}
  CreateDirectory "$INSTDIR"
  Call WaitForExit

  ; 여기부터 $INSTDIR 는 없었거나 비었거나($DirKind 0), 워십프리젠터 설치로 확인된 폴더($DirKind 1)다.
  ; Electron 판 파일 정리 (Rust 판은 쓰지 않음): 워십프리젠터 Electron 판으로 확인된 폴더에서만, 이름을 아는 파일만 지운다.
  ; (*.dll·*.pak·*.bin 묶음 삭제를 하지 않는다 — 사용자가 둔 파일이 남는다)
  ${If} $DirKind == 1
  ${AndIf} ${FileExists} "$INSTDIR\resources\app.asar"
    Delete "$INSTDIR\locales\*.pak" ; Electron 언어 파일 (ko.pak, en-US.pak …)
    RMDir "$INSTDIR\locales"
    Delete "$INSTDIR\resources\app.asar"
    Delete "$INSTDIR\resources\app-update.yml"
    Delete "$INSTDIR\resources\elevate.exe"
    Delete "$INSTDIR\resources\LICENSE.txt"
    Delete "$INSTDIR\resources\THIRD_PARTY_NOTICES.md"
    Delete "$INSTDIR\resources\bible.db"
    RMDir /r "$INSTDIR\resources\app.asar.unpacked"
    RMDir "$INSTDIR\resources"
    Delete "$INSTDIR\chrome_100_percent.pak"
    Delete "$INSTDIR\chrome_200_percent.pak"
    Delete "$INSTDIR\resources.pak"
    Delete "$INSTDIR\snapshot_blob.bin"
    Delete "$INSTDIR\v8_context_snapshot.bin"
    Delete "$INSTDIR\d3dcompiler_47.dll"
    Delete "$INSTDIR\dxcompiler.dll"
    Delete "$INSTDIR\dxil.dll"
    Delete "$INSTDIR\ffmpeg.dll"
    Delete "$INSTDIR\libEGL.dll"
    Delete "$INSTDIR\libGLESv2.dll"
    Delete "$INSTDIR\vk_swiftshader.dll"
    Delete "$INSTDIR\vulkan-1.dll"
    Delete "$INSTDIR\icudtl.dat"
    Delete "$INSTDIR\LICENSE.electron.txt"
    Delete "$INSTDIR\LICENSES.chromium.html"
    Delete "$INSTDIR\vk_swiftshader_icd.json"
  ${EndIf}
  ; 앞 판(Rust)이 넣은 파일: 그 판의 파일 목록에 적힌 것만 지운다 (web\ 을 통째로 지우지 않는다)
  Call RemoveListedFiles

  !insertmacro WP_EACH_FILE WP_INSTALL_FILE
  SetOutPath "$INSTDIR"

  ; 이번에 넣은 파일 목록
  ClearErrors
  FileOpen $R0 "$INSTDIR\${MANIFEST}" w
  ${IfNot} ${Errors}
    !insertmacro WP_EACH_FILE WP_LIST_FILE
    !insertmacro WP_EACH_DIR WP_LIST_DIR
    FileClose $R0
  ${EndIf}

  WriteUninstaller "$INSTDIR\${UNEXE}"

  ${If} $TestMode == 0
    ${If} $PerMachine == 1
      ${If} $RegView64 == 1
        SetRegView 64
      ${EndIf}
      !insertmacro WriteInstallReg HKLM "/allusers"
      SetRegView 32
    ${Else}
      !insertmacro WriteInstallReg HKCU "/currentuser"
    ${EndIf}

    ; 바로가기: 같은 이름으로 덮어써 아이콘까지 새로 (위치·이름은 Electron 판과 같음)
    CreateShortcut "$DESKTOP\${SHORTCUT}.lnk" "$INSTDIR\${EXE}" "" "$INSTDIR\WorshipPresenter.ico" 0
    CreateShortcut "$SMPROGRAMS\${SHORTCUT}.lnk" "$INSTDIR\${EXE}" "" "$INSTDIR\WorshipPresenter.ico" 0

    ; Electron 판 자동 업데이트가 남긴 옛 설치 파일(100MB 넘음). 지금 실행 중인 설치 파일(pending\)은 못 지우므로
    ; 프로그램이 다음에 켜질 때 마저 지운다. 관리자 권한으로 다시 실행된 쪽은 다른 계정의 폴더일 수 있어 건드리지 않는다
    ${If} $ElevatedChild == 0
      Delete "$LOCALAPPDATA\worshipcue-updater\installer.exe"
      Delete "$LOCALAPPDATA\worshipcue-updater\current.blockmap"
    ${EndIf}
  ${EndIf}

  ${If} $ForceRun == 1
  ${AndIf} $ElevatedChild == 0
    Exec '"$INSTDIR\${EXE}"'
  ${Else}
    ; 새 실행 파일을 처음 켤 때는 백신 검사로 2초쯤 아무것도 안 보인다. 설치하는 김에 미리 한 번 켰다 끄게 해(창 없음, 바로 끝남)
    ; 사용자가 아이콘을 처음 누를 때는 바로 뜨게 한다
    Exec '"$INSTDIR\${EXE}" --warmup'
    Exec '"$INSTDIR\wp-output.exe" --list'
  ${EndIf}
SectionEnd

Function un.onInit
  StrCpy $PerMachine 0
  !insertmacro FindMachineInstall
  ${If} $0 == $INSTDIR
    StrCpy $PerMachine 1
    UserInfo::GetAccountType
    Pop $1
    ${If} $1 != "Admin"
      ; 모든 사용자용 설치를 지우려면 관리자 권한이 필요하다
      StrCpy $2 "/allusers"
      ${If} ${Silent}
        StrCpy $2 "$2 /S"
      ${EndIf}
      ExecShell "runas" "$INSTDIR\${UNEXE}" "$2"
      Quit
    ${EndIf}
  ${EndIf}
FunctionEnd

Section "Uninstall"
  ${If} $PerMachine == 1
    SetShellVarContext all
  ${Else}
    SetShellVarContext current
  ${EndIf}
  ; 켜져 있으면 실행 파일은 못 지우고 web\ 만 지워져 반쯤 남는다 → 아무것도 지우기 전에 그만둔다
  StrCpy $3 0
  !insertmacro CheckLocked "$INSTDIR\${EXE}"
  !insertmacro CheckLocked "$INSTDIR\wp-output.exe"
  ${If} $3 == 1
    ${IfNot} ${Silent}
      MessageBox MB_OK|MB_ICONEXCLAMATION "워십프리젠터가 켜져 있습니다.$\r$\n프로그램을 끈 뒤 다시 제거해 주세요."
    ${EndIf}
    SetErrorLevel 2
    Abort "워십프리젠터가 켜져 있어 제거하지 못했습니다."
  ${EndIf}
  ; 이 판이 넣은 파일만 지운다 (*.dll 묶음 삭제나 web\ 통째 삭제를 하지 않는다). 사용자가 둔 파일이 있으면 폴더와 함께 남는다
  !insertmacro WP_EACH_FILE WP_DELETE_FILE
  !insertmacro WP_EACH_DIR WP_REMOVE_DIR
  Delete "$INSTDIR\${MANIFEST}"
  Delete "$INSTDIR\${UNEXE}"
  RMDir "$INSTDIR"
  ; 바로가기·제거 항목은 '등록된 설치'를 지울 때만 지운다.
  ; (/TEST /D= 로 따로 풀어 본 것이나 다른 폴더의 사본을 지울 때 실제 설치의 바로가기까지 지우지 않게)
  ${If} $PerMachine == 1
    Delete "$DESKTOP\${SHORTCUT}.lnk"
    Delete "$SMPROGRAMS\${SHORTCUT}.lnk"
    ${If} $RegView64 == 1
      SetRegView 64
    ${EndIf}
    DeleteRegKey HKLM "${UNKEY}"
    DeleteRegKey HKLM "${APPKEY}"
    SetRegView 32
  ${Else}
    ReadRegStr $0 HKCU "${APPKEY}" "InstallLocation"
    ${If} $0 == $INSTDIR
      Delete "$DESKTOP\${SHORTCUT}.lnk"
      Delete "$SMPROGRAMS\${SHORTCUT}.lnk"
      DeleteRegKey HKCU "${UNKEY}"
      DeleteRegKey HKCU "${APPKEY}"
    ${EndIf}
  ${EndIf}
  ; 예배 자료(%APPDATA%\WorshipPresenter-rs, %APPDATA%\WorshipCue)는 지우지 않는다
SectionEnd