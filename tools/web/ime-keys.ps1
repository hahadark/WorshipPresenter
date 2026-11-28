# [웹판] 실제 Windows 한국어 입력기(IME)로 브라우저 창에 키를 누른다 (점검용).
# CDP 의 글 넣기 · 조합 흉내와 달리, 운영체제 키 입력(SendInput 가상 키)이 입력기를 거쳐 한글로 조합된다.
# powershell -File tools/web/ime-keys.ps1 -BrowserPid <pid> -Actions "hangul|keys:dyrhk|key:BACK|wait:200|click:100,60|ctrl:V"
#   keys:<글자들>  영문 자판 글자를 차례로 (입력기가 한글 모드면 두벌식으로 조합된다). ':' 는 Shift+;
#   key:<이름>     BACK RETURN SPACE ESCAPE TAB HANGUL LEFT RIGHT UP DOWN HOME END DELETE F5 F6 F9 F10 F11
#   ctrl:<글자>    Ctrl+글자 (V 붙여 넣기 · A 모두 고르기 · Z 되돌리기)
#   shift:<이름>   Shift+키
#   hangul / english  입력기 한/영 상태를 맞춘다 (지금 상태를 읽고 다르면 한/영 키)
#   click:x,y     창 안(페이지) 좌표를 누른다 (CSS px, 배율 곱함) · dblclick:x,y
#   wait:<ms>     mode   (지금 입력기 상태를 출력)
#   clip:<글>      클립보드에 글을 넣는다 (\n 은 줄바꿈)
param([int]$BrowserPid, [string]$Actions, [double]$Scale = 1.0, [string]$Title = "")
$ErrorActionPreference = 'Stop'
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
using System.Text;
public static class K {
  [StructLayout(LayoutKind.Sequential)] public struct KEYBDINPUT { public ushort wVk; public ushort wScan; public uint dwFlags; public uint time; public IntPtr dwExtraInfo; }
  [StructLayout(LayoutKind.Sequential)] public struct MOUSEINPUT { public int dx; public int dy; public uint mouseData; public uint dwFlags; public uint time; public IntPtr dwExtraInfo; }
  [StructLayout(LayoutKind.Explicit)] public struct U { [FieldOffset(0)] public KEYBDINPUT ki; [FieldOffset(0)] public MOUSEINPUT mi; }
  [StructLayout(LayoutKind.Sequential)] public struct INPUT { public uint type; public U u; }
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int l, t, r, b; }
  [StructLayout(LayoutKind.Sequential)] public struct POINT { public int x, y; }
  [StructLayout(LayoutKind.Sequential)] public struct GUITHREADINFO { public int cbSize; public int flags; public IntPtr hwndActive; public IntPtr hwndFocus; public IntPtr hwndCapture; public IntPtr hwndMenuOwner; public IntPtr hwndMoveSize; public IntPtr hwndCaret; public RECT rcCaret; }
  public delegate bool EnumProc(IntPtr h, IntPtr l);
  [DllImport("user32.dll")] public static extern uint SendInput(uint n, INPUT[] i, int size);
  [DllImport("user32.dll")] public static extern uint MapVirtualKey(uint c, uint t);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc p, IntPtr l);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern int GetWindowText(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern int GetClassName(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int c);
  [DllImport("user32.dll")] public static extern bool BringWindowToTop(IntPtr h);
  [DllImport("user32.dll")] public static extern bool AttachThreadInput(uint a, uint b, bool f);
  [DllImport("kernel32.dll")] public static extern uint GetCurrentThreadId();
  [DllImport("user32.dll")] public static extern bool GetGUIThreadInfo(uint tid, ref GUITHREADINFO g);
  [DllImport("user32.dll")] public static extern bool ClientToScreen(IntPtr h, ref POINT p);
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern void keybd_event(byte vk, byte scan, uint flags, IntPtr extra);
  [DllImport("imm32.dll")] public static extern IntPtr ImmGetDefaultIMEWnd(IntPtr h);
  [DllImport("user32.dll")] public static extern IntPtr SendMessage(IntPtr h, uint msg, IntPtr w, IntPtr l);
  [DllImport("user32.dll")] public static extern IntPtr PostMessage(IntPtr h, uint msg, IntPtr w, IntPtr l);
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  [DllImport("user32.dll")] public static extern bool GetCursorPos(out POINT p);
  [DllImport("user32.dll")] public static extern IntPtr WindowFromPoint(POINT p);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  public static IntPtr Find(uint pid, string title) {
    IntPtr found = IntPtr.Zero;
    EnumWindows((h, l) => {
      uint p; GetWindowThreadProcessId(h, out p);
      if (p != pid || !IsWindowVisible(h)) return true;
      var c = new StringBuilder(256); GetClassName(h, c, 256);
      if (c.ToString() != "Chrome_WidgetWin_1") return true;
      var t = new StringBuilder(512); GetWindowText(h, t, 512);
      if (t.Length == 0 || (title.Length > 0 && (!t.ToString().StartsWith(title) || t.ToString().Contains("출력")))) return true; // 제목이 주어지면 그 창만 (출력 창 말고 조작 화면)
      found = h; return false;
    }, IntPtr.Zero);
    return found;
  }
  public static void Key(ushort vk, bool up) {
    var i = new INPUT[1]; i[0].type = 1; i[0].u.ki.wVk = vk; i[0].u.ki.wScan = (ushort)MapVirtualKey(vk, 0);
    uint f = up ? 2u : 0u;
    if (vk == 0x21 || vk == 0x22 || vk == 0x25 || vk == 0x26 || vk == 0x27 || vk == 0x28 || vk == 0x2E || vk == 0x24 || vk == 0x23) f |= 1; // 확장 키
    i[0].u.ki.dwFlags = f;
    SendInput(1, i, Marshal.SizeOf(typeof(INPUT)));
  }
  [DllImport("user32.dll")] public static extern int GetSystemMetrics(int i);
  public static void MoveAbs(int x, int y) {
    var i = new INPUT[1]; i[0].type = 0; i[0].u.mi.dwFlags = 0x8001; // MOVE | ABSOLUTE (0~65535)
    i[0].u.mi.dx = (int)((x * 65535.0) / (GetSystemMetrics(0) - 1)); i[0].u.mi.dy = (int)((y * 65535.0) / (GetSystemMetrics(1) - 1));
    SendInput(1, i, Marshal.SizeOf(typeof(INPUT)));
  }
  public static void Mouse(uint flags) {
    var i = new INPUT[1]; i[0].type = 0; i[0].u.mi.dwFlags = flags;
    SendInput(1, i, Marshal.SizeOf(typeof(INPUT)));
  }
  public static void Focus(IntPtr h, bool alt) {
    uint pid; uint target = GetWindowThreadProcessId(h, out pid);
    uint fg = GetWindowThreadProcessId(GetForegroundWindow(), out pid);
    uint me = GetCurrentThreadId();
    // Alt 한 번: 다른 프로그램 창을 앞으로 가져올 수 있게. 이미 브라우저가 앞이면 누르지 않는다 (앱 창의 메뉴가 열려 다음 키를 가져감)
    if (alt) { keybd_event(0x12, 0, 0, IntPtr.Zero); keybd_event(0x12, 0, 2, IntPtr.Zero); }
    AttachThreadInput(me, fg, true); AttachThreadInput(me, target, true);
    ShowWindow(h, 9); BringWindowToTop(h); SetForegroundWindow(h);
    AttachThreadInput(me, fg, false); AttachThreadInput(me, target, false);
  }
  public static int ImeMode(IntPtr top) {
    uint pid; uint tid = GetWindowThreadProcessId(top, out pid);
    var g = new GUITHREADINFO(); g.cbSize = Marshal.SizeOf(typeof(GUITHREADINFO));
    GetGUIThreadInfo(tid, ref g);
    IntPtr f = g.hwndFocus != IntPtr.Zero ? g.hwndFocus : top;
    IntPtr ime = ImmGetDefaultIMEWnd(f);
    return (int)SendMessage(ime, 0x283, (IntPtr)1, IntPtr.Zero); // WM_IME_CONTROL, IMC_GETCONVERSIONMODE (1 = 한글)
  }
}
'@
[void][K]::SetProcessDPIAware() # 화면 배율이 100% 가 아니어도 좌표가 실제 화소가 되게
$h = [K]::Find([uint32]$BrowserPid, $Title)
if ($h -eq [IntPtr]::Zero) { throw "브라우저 창(pid $BrowserPid)을 찾지 못했습니다" }
if ([K]::GetForegroundWindow() -ne $h) { [K]::Focus($h, $false); Start-Sleep -Milliseconds 300 }
if ([K]::GetForegroundWindow() -ne $h) { [K]::Focus($h, $true); Start-Sleep -Milliseconds 300 }
"foreground=" + ([K]::GetForegroundWindow() -eq $h)
[void][K]::PostMessage($h, 0x50, [IntPtr]0, [IntPtr]0x04120412) # 한국어 자판으로
Start-Sleep -Milliseconds 100
$VK = @{ PAGEUP = 0x21; PAGEDOWN = 0x22; BACK = 8; TAB = 9; RETURN = 13; SHIFT = 0x10; CONTROL = 0x11; ESCAPE = 0x1B; SPACE = 0x20; HANGUL = 0x15; HANJA = 0x19; LEFT = 0x25; UP = 0x26; RIGHT = 0x27; DOWN = 0x28; HOME = 0x24; END = 0x23; DELETE = 0x2E; F5 = 0x74; F6 = 0x75; F9 = 0x78; F10 = 0x79; F11 = 0x7A }
function Press([int]$vk) { [K]::Key($vk, $false); Start-Sleep -Milliseconds 25; [K]::Key($vk, $true); Start-Sleep -Milliseconds 45 }
function PressChar([char]$c) {
  switch -CaseSensitive ($c) {
    ' ' { Press 0x20; return }
    ':' { [K]::Key(0x10, $false); Press 0xBA; [K]::Key(0x10, $true); return }
    ';' { Press 0xBA; return }
    '-' { Press 0xBD; return }
    '.' { Press 0xBE; return }
    ',' { Press 0xBC; return }
    default {
      if ($c -cmatch '[A-Z]') { [K]::Key(0x10, $false); Press ([int][char]$c); [K]::Key(0x10, $true); return }
      if ($c -cmatch '[a-z]') { Press ([int][char]::ToUpper($c)); return }
      if ($c -cmatch '[0-9]') { Press ([int]$c); return }
      throw "모르는 글자: $c"
    }
  }
}
function Click([string]$xy, [int]$n) {
  $p = $xy.Split(','); $pt = New-Object K+POINT; $pt.x = [int]([double]$p[0] * $Scale); $pt.y = [int]([double]$p[1] * $Scale)
  [void][K]::ClientToScreen($h, [ref]$pt)
  # 실제 마우스처럼 옆에서 다가와 멈춘 뒤 누른다 (커서만 옮기면 브라우저가 포인터 이동을 못 받을 때가 있다)
  [K]::MoveAbs($pt.x - 6, $pt.y - 3); Start-Sleep -Milliseconds 40; [K]::MoveAbs($pt.x - 2, $pt.y - 1); Start-Sleep -Milliseconds 40; [K]::MoveAbs($pt.x, $pt.y); Start-Sleep -Milliseconds 80
  if ($env:WP_IME_DEBUG) { $cp = New-Object K+POINT; [void][K]::GetCursorPos([ref]$cp); $wr = New-Object K+RECT; [void][K]::GetWindowRect($h, [ref]$wr); "click want=$($pt.x),$($pt.y) cursor=$($cp.x),$($cp.y) win=$($wr.l),$($wr.t),$($wr.r),$($wr.b) under=$([K]::WindowFromPoint($cp)) top=$h" }
  for ($i = 0; $i -lt $n; $i++) { [K]::Mouse(2); Start-Sleep -Milliseconds 30; [K]::Mouse(4); Start-Sleep -Milliseconds 60 }
}
foreach ($a in $Actions.Split('|')) {
  if ([K]::GetForegroundWindow() -ne $h) { [K]::Focus($h, $false); Start-Sleep -Milliseconds 200 }
  $cmd, $arg = $a.Split(':', 2)
  switch ($cmd) {
    'keys' { foreach ($c in $arg.ToCharArray()) { PressChar $c } }
    'key' { Press $VK[$arg] }
    'ctrl' { [K]::Key(0x11, $false); if ($VK.ContainsKey($arg)) { Press $VK[$arg] } else { Press ([int][char]$arg.ToUpper()) }; [K]::Key(0x11, $true); Start-Sleep -Milliseconds 60 }
    'shift' { [K]::Key(0x10, $false); Press $VK[$arg]; [K]::Key(0x10, $true) }
    'alt' { [K]::Key(0x12, $false); Press $VK[$arg]; [K]::Key(0x12, $true) }
    'hangul' { if (([K]::ImeMode($h) -band 1) -eq 0) { Press 0x15; Start-Sleep -Milliseconds 150 }; "mode=" + [K]::ImeMode($h) }
    'english' { if (([K]::ImeMode($h) -band 1) -ne 0) { Press 0x15; Start-Sleep -Milliseconds 150 }; "mode=" + [K]::ImeMode($h) }
    'mode' { "mode=" + [K]::ImeMode($h) }
    'click' { Click $arg 1 }
    'dblclick' { Click $arg 2 }
    'wait' { Start-Sleep -Milliseconds ([int]$arg) }
    'clip' { Set-Clipboard -Value ($arg -replace '\\n', "`r`n") }
    default { throw "모르는 동작: $a" }
  }
}
