param([int]$ProcId, [string]$Steps)
if ($Steps.StartsWith('@')) { $Steps = Get-Content -Raw -Encoding UTF8 $Steps.Substring(1) }; $Do = $Steps.Trim() -split ';;'
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes
$root = [System.Windows.Automation.AutomationElement]::RootElement
$cond = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::ProcessIdProperty, $ProcId)
$win = $root.FindFirst([System.Windows.Automation.TreeScope]::Children, $cond)
if (-not $win) { "창 없음"; exit 1 }
function Find([string]$name, [int]$nth = 0) {
  $c = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::NameProperty, $name)
  $all = $win.FindAll([System.Windows.Automation.TreeScope]::Descendants, $c)
  if ($all.Count -le $nth) { return $null }
  return $all[$nth]
}
Add-Type -Namespace W -Name M -MemberDefinition '[DllImport("user32.dll")] public static extern bool SetProcessDPIAware(); [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y); [DllImport("user32.dll")] public static extern void mouse_event(int f, int x, int y, int d, int e);'
[void][W.M]::SetProcessDPIAware()
function ClickEl($e) { $r = $e.Current.BoundingRectangle; [void][W.M]::SetCursorPos([int]($r.X + $r.Width/2), [int]($r.Y + $r.Height/2)); Start-Sleep -Milliseconds 120; [W.M]::mouse_event(2,0,0,0,0); Start-Sleep -Milliseconds 60; [W.M]::mouse_event(4,0,0,0,0) }
foreach ($a in $Do) {
  $t = $a.Split('|')
  switch ($t[0]) {
    'list' { $all = $win.FindAll([System.Windows.Automation.TreeScope]::Descendants, [System.Windows.Automation.Condition]::TrueCondition); "요소 $($all.Count)개"; $all | Select-Object -First ([int]$t[1]) | ForEach-Object { "  [$($_.Current.ControlType.ProgrammaticName)] $($_.Current.Name)" } }
    'press' { $e = Find $t[1] ([int]($t[2] | ForEach-Object { if ($_) { $_ } else { 0 } })); if (-not $e) { "없음: $($t[1])"; continue }; try { $e.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke(); "누름: $($t[1])" } catch { try { $e.GetCurrentPattern([System.Windows.Automation.TogglePattern]::Pattern).Toggle(); "토글: $($t[1])" } catch { "못 누름: $($t[1]) $_" } }; Start-Sleep -Milliseconds 400 }
    'set' { $e = Find $t[1] ([int]$t[3]); if (-not $e) { "없음: $($t[1])"; continue }; try { $e.GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern).SetValue($t[2]); "값: $($t[1]) = $($t[2])" } catch { "못 넣음: $($t[1]) $_" }; Start-Sleep -Milliseconds 400 }
    'edit' { $c = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::ControlTypeProperty, [System.Windows.Automation.ControlType]::Edit); $all = $win.FindAll([System.Windows.Automation.TreeScope]::Descendants, $c); $e = $all[[int]$t[1]]; try { $e.GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern).SetValue($t[2]); "값 넣음 #$($t[1]) = $($t[2])" } catch { "못 넣음 #$($t[1]) $_" }; Start-Sleep -Milliseconds 400 }
    'custom' { $c = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::ControlTypeProperty, [System.Windows.Automation.ControlType]::Custom); $all = $win.FindAll([System.Windows.Automation.TreeScope]::Descendants, $c); $e = $all[[int]$t[1]]; try { $e.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke(); "누름 custom #$($t[1])" } catch { try { $e.SetFocus(); $r = $e.Current.BoundingRectangle; "초점 custom #$($t[1])" } catch { "못 누름 custom #$($t[1]) $_" } }; Start-Sleep -Milliseconds 500 }
    'keys' { $w = New-Object -ComObject WScript.Shell; try { $win.SetFocus() } catch {}; [void]$w.AppActivate('워십프리젠터 (Rust)'); Start-Sleep -Milliseconds 300; $w.SendKeys($t[1]); "키: $($t[1])"; Start-Sleep -Milliseconds 500 }
    'combo' { $c = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::ControlTypeProperty, [System.Windows.Automation.ControlType]::ComboBox); $all = $win.FindAll([System.Windows.Automation.TreeScope]::Descendants, $c); $e = $all[[int]$t[1]]; try { $e.GetCurrentPattern([System.Windows.Automation.ExpandCollapsePattern]::Pattern).Expand(); "펼침 combo #$($t[1])" } catch { try { $e.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke(); "누름 combo #$($t[1])" } catch { "못 엶 combo #$($t[1]) $_" } }; Start-Sleep -Milliseconds 600 }
    'clickcombo' { $c = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::ControlTypeProperty, [System.Windows.Automation.ControlType]::ComboBox); $all = $win.FindAll([System.Windows.Automation.TreeScope]::Descendants, $c); ClickEl $all[[int]$t[1]]; "클릭 combo #$($t[1])"; Start-Sleep -Milliseconds 600 }
    'click' { $e = Find $t[1] ([int]($t[2] | ForEach-Object { if ($_) { $_ } else { 0 } })); if (-not $e) { "없음: $($t[1])"; continue }; ClickEl $e; "클릭: $($t[1])"; Start-Sleep -Milliseconds 600 }
    'clickpart' { $all = $win.FindAll([System.Windows.Automation.TreeScope]::Descendants, [System.Windows.Automation.Condition]::TrueCondition); $e = $all | Where-Object { $_.Current.Name -like "*$($t[1])*" } | Select-Object -First 1; if (-not $e) { "없음(부분): $($t[1])"; continue }; ClickEl $e; "클릭(부분): $($e.Current.Name)"; Start-Sleep -Milliseconds 700 }
    'rclick' { $e = Find $t[1] ([int]($t[2] | ForEach-Object { if ($_) { $_ } else { 0 } })); if (-not $e) { "없음: $($t[1])"; continue }; $r = $e.Current.BoundingRectangle; [void][W.M]::SetCursorPos([int]($r.X + $r.Width/2), [int]($r.Y + $r.Height/2)); Start-Sleep -Milliseconds 120; [W.M]::mouse_event(8,0,0,0,0); Start-Sleep -Milliseconds 60; [W.M]::mouse_event(16,0,0,0,0); "오른쪽 클릭: $($t[1])"; Start-Sleep -Milliseconds 600 }
    'wait' { Start-Sleep -Milliseconds ([int]$t[1]) }
  }
}



