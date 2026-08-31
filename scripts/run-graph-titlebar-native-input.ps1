param(
  [Parameter(Mandatory = $true)][int]$ProcessId,
  [ValidateSet('drag', 'alt-tab', 'release-mouse')][string]$Mode = 'drag',
  [int]$Samples = 112,
  [int]$StepX = 2,
  [int]$StepY = 1,
  [int]$CadenceMs = 16
)

$ErrorActionPreference = 'Stop'

Add-Type @'
using System;
using System.Runtime.InteropServices;

public static class GraphTitlebarNativeInput {
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left, Top, Right, Bottom; }
  [StructLayout(LayoutKind.Sequential)] public struct POINT { public int X, Y; }
  [StructLayout(LayoutKind.Sequential)] public struct INPUT { public uint Type; public MOUSEKEYBDHARDWAREINPUT Data; }
  [StructLayout(LayoutKind.Explicit)] public struct MOUSEKEYBDHARDWAREINPUT {
    [FieldOffset(0)] public MOUSEINPUT Mouse;
    [FieldOffset(0)] public KEYBDINPUT Keyboard;
  }
  [StructLayout(LayoutKind.Sequential)] public struct MOUSEINPUT {
    public int Dx, Dy; public uint MouseData, Flags, Time; public IntPtr ExtraInfo;
  }
  [StructLayout(LayoutKind.Sequential)] public struct KEYBDINPUT {
    public ushort VirtualKey, Scan; public uint Flags, Time; public IntPtr ExtraInfo;
  }

  public const uint INPUT_MOUSE = 0;
  public const uint INPUT_KEYBOARD = 1;
  public const uint MOUSEEVENTF_MOVE = 0x0001;
  public const uint MOUSEEVENTF_LEFTDOWN = 0x0002;
  public const uint MOUSEEVENTF_LEFTUP = 0x0004;
  public const uint MOUSEEVENTF_ABSOLUTE = 0x8000;
  public const uint KEYEVENTF_KEYUP = 0x0002;
  public const ushort VK_MENU = 0x12;
  public const ushort VK_TAB = 0x09;

  [DllImport("user32.dll", SetLastError=true)] public static extern uint SendInput(uint count, INPUT[] inputs, int size);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr handle, out RECT rect);
  [DllImport("user32.dll")] public static extern bool MoveWindow(IntPtr handle, int x, int y, int width, int height, bool repaint);
  [DllImport("user32.dll")] public static extern bool GetCursorPos(out POINT point);
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr handle, out uint processId);
  [DllImport("user32.dll")] public static extern int GetSystemMetrics(int index);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr handle);
  [DllImport("user32.dll", SetLastError=true)] public static extern bool SetWindowPos(IntPtr handle, IntPtr insertAfter, int x, int y, int width, int height, uint flags);
  [DllImport("user32.dll")] public static extern uint GetDpiForWindow(IntPtr handle);

  public static readonly IntPtr HWND_TOPMOST = new IntPtr(-1);
  public static readonly IntPtr HWND_NOTOPMOST = new IntPtr(-2);
  public const uint SWP_NOSIZE = 0x0001;
  public const uint SWP_NOMOVE = 0x0002;
  public const uint SWP_SHOWWINDOW = 0x0040;

  public static void Mouse(uint flags, int x, int y) {
    int width = Math.Max(1, GetSystemMetrics(0) - 1);
    int height = Math.Max(1, GetSystemMetrics(1) - 1);
    var input = new INPUT { Type = INPUT_MOUSE };
    input.Data.Mouse = new MOUSEINPUT {
      Dx = (int)Math.Round(x * 65535.0 / width),
      Dy = (int)Math.Round(y * 65535.0 / height),
      Flags = flags | MOUSEEVENTF_ABSOLUTE,
      ExtraInfo = IntPtr.Zero
    };
    if (SendInput(1, new [] { input }, Marshal.SizeOf(typeof(INPUT))) != 1) {
      throw new System.ComponentModel.Win32Exception(Marshal.GetLastWin32Error());
    }
  }

  public static void Key(ushort key, bool up) {
    var input = new INPUT { Type = INPUT_KEYBOARD };
    input.Data.Keyboard = new KEYBDINPUT { VirtualKey = key, Flags = up ? KEYEVENTF_KEYUP : 0 };
    if (SendInput(1, new [] { input }, Marshal.SizeOf(typeof(INPUT))) != 1) {
      throw new System.ComponentModel.Win32Exception(Marshal.GetLastWin32Error());
    }
  }
}
'@

function Get-MainWindowHandle([int]$TargetProcessId) {
  $deadline = [DateTime]::UtcNow.AddSeconds(15)
  while ([DateTime]::UtcNow -lt $deadline) {
    $candidate = Get-Process -Id $TargetProcessId -ErrorAction SilentlyContinue
    if ($candidate -and $candidate.MainWindowHandle -ne [IntPtr]::Zero) {
      return $candidate.MainWindowHandle
    }
    Start-Sleep -Milliseconds 50
  }
  throw "No native main window was found for process $TargetProcessId."
}

function Get-ForegroundSnapshot {
  $foreground = [GraphTitlebarNativeInput]::GetForegroundWindow()
  [uint32]$foregroundPid = 0
  if ($foreground -ne [IntPtr]::Zero) {
    [GraphTitlebarNativeInput]::GetWindowThreadProcessId($foreground, [ref]$foregroundPid) | Out-Null
  }
  return [pscustomobject]@{
    hwnd = $foreground.ToInt64()
    processId = [int]$foregroundPid
  }
}

if ($Mode -eq 'release-mouse') {
  [GraphTitlebarNativeInput]::Mouse([GraphTitlebarNativeInput]::MOUSEEVENTF_LEFTUP, 0, 0)
  [pscustomobject]@{ mode = 'release-mouse'; released = $true } | ConvertTo-Json -Compress
  exit 0
}

$handle = Get-MainWindowHandle $ProcessId
[GraphTitlebarNativeInput]::SetForegroundWindow($handle) | Out-Null
Start-Sleep -Milliseconds 250

if ($Mode -eq 'alt-tab') {
  Add-Type -AssemblyName System.Windows.Forms
  $control = New-Object System.Windows.Forms.Form
  $control.Text = 'Graph titlebar Alt-Tab control'
  $control.StartPosition = [System.Windows.Forms.FormStartPosition]::Manual
  $control.Left = 48
  $control.Top = 48
  $control.Width = 360
  $control.Height = 180
  $control.ShowInTaskbar = $true
  try {
    $controlDeadline = [DateTime]::UtcNow.AddSeconds(10)
    $control.Show()
    while ([DateTime]::UtcNow -lt $controlDeadline -and $control.Handle -eq [IntPtr]::Zero) {
      [System.Windows.Forms.Application]::DoEvents()
      Start-Sleep -Milliseconds 50
    }
    if ($control.Handle -eq [IntPtr]::Zero) { throw 'Alt-Tab control window was unavailable.' }
    $control.TopMost = $true
    $controlCenterX = $control.Left + [int]($control.Width / 2)
    $controlCenterY = $control.Top + [int]($control.Height / 2)
    [GraphTitlebarNativeInput]::Mouse([GraphTitlebarNativeInput]::MOUSEEVENTF_MOVE, $controlCenterX, $controlCenterY)
    [GraphTitlebarNativeInput]::Mouse([GraphTitlebarNativeInput]::MOUSEEVENTF_LEFTDOWN, $controlCenterX, $controlCenterY)
    [GraphTitlebarNativeInput]::Mouse([GraphTitlebarNativeInput]::MOUSEEVENTF_LEFTUP, $controlCenterX, $controlCenterY)
    $controlForegroundDeadline = [DateTime]::UtcNow.AddMilliseconds(500)
    while ([DateTime]::UtcNow -lt $controlForegroundDeadline) {
      [System.Windows.Forms.Application]::DoEvents()
      Start-Sleep -Milliseconds 25
    }
    $controlForeground = Get-ForegroundSnapshot
    if ($controlForeground.processId -ne $PID) { throw "The Alt-Tab control window was not activated by native input. controlForeground=$(($controlForeground | ConvertTo-Json -Compress)) pid=$PID target=$ProcessId" }
    $control.TopMost = $false
    $zOrderFlags = [GraphTitlebarNativeInput]::SWP_NOSIZE -bor [GraphTitlebarNativeInput]::SWP_NOMOVE -bor [GraphTitlebarNativeInput]::SWP_SHOWWINDOW
    [GraphTitlebarNativeInput]::SetWindowPos($handle, [GraphTitlebarNativeInput]::HWND_TOPMOST, 0, 0, 0, 0, $zOrderFlags) | Out-Null
    $targetRect = New-Object GraphTitlebarNativeInput+RECT
    if (-not [GraphTitlebarNativeInput]::GetWindowRect($handle, [ref]$targetRect)) { throw 'GetWindowRect failed before target activation.' }
    $targetTitlebarX = $targetRect.Left + [int](($targetRect.Right - $targetRect.Left) / 2)
    $targetTitlebarY = $targetRect.Top + 16
    [GraphTitlebarNativeInput]::Mouse([GraphTitlebarNativeInput]::MOUSEEVENTF_MOVE, $targetTitlebarX, $targetTitlebarY)
    [GraphTitlebarNativeInput]::Mouse([GraphTitlebarNativeInput]::MOUSEEVENTF_LEFTDOWN, $targetTitlebarX, $targetTitlebarY)
    [GraphTitlebarNativeInput]::Mouse([GraphTitlebarNativeInput]::MOUSEEVENTF_LEFTUP, $targetTitlebarX, $targetTitlebarY)
    $targetForegroundDeadline = [DateTime]::UtcNow.AddMilliseconds(500)
    while ([DateTime]::UtcNow -lt $targetForegroundDeadline) {
      [System.Windows.Forms.Application]::DoEvents()
      Start-Sleep -Milliseconds 25
    }
    $before = Get-ForegroundSnapshot
    if ($before.processId -ne $ProcessId) { throw "The tested app was not foreground before Alt-Tab. before=$(($before | ConvertTo-Json -Compress)) target=$ProcessId targetHwnd=$($handle.ToInt64()) controlHwnd=$($control.Handle.ToInt64()) clickX=$targetTitlebarX clickY=$targetTitlebarY" }
    [GraphTitlebarNativeInput]::SetWindowPos($handle, [GraphTitlebarNativeInput]::HWND_NOTOPMOST, 0, 0, 0, 0, $zOrderFlags) | Out-Null
    [GraphTitlebarNativeInput]::Key([GraphTitlebarNativeInput]::VK_MENU, $false)
    [GraphTitlebarNativeInput]::Key([GraphTitlebarNativeInput]::VK_TAB, $false)
    [GraphTitlebarNativeInput]::Key([GraphTitlebarNativeInput]::VK_TAB, $true)
    [GraphTitlebarNativeInput]::Key([GraphTitlebarNativeInput]::VK_MENU, $true)
    $awayDeadline = [DateTime]::UtcNow.AddMilliseconds(800)
    while ([DateTime]::UtcNow -lt $awayDeadline) {
      [System.Windows.Forms.Application]::DoEvents()
      Start-Sleep -Milliseconds 25
    }
    $away = Get-ForegroundSnapshot
    [GraphTitlebarNativeInput]::Key([GraphTitlebarNativeInput]::VK_MENU, $false)
    [GraphTitlebarNativeInput]::Key([GraphTitlebarNativeInput]::VK_TAB, $false)
    [GraphTitlebarNativeInput]::Key([GraphTitlebarNativeInput]::VK_TAB, $true)
    [GraphTitlebarNativeInput]::Key([GraphTitlebarNativeInput]::VK_MENU, $true)
    $returnDeadline = [DateTime]::UtcNow.AddMilliseconds(800)
    while ([DateTime]::UtcNow -lt $returnDeadline) {
      [System.Windows.Forms.Application]::DoEvents()
      Start-Sleep -Milliseconds 25
    }
    $returned = Get-ForegroundSnapshot
    [pscustomobject]@{ mode = 'send-input-alt-tab'; targetProcessId = $ProcessId; targetHwnd = $handle.ToInt64(); before = $before; away = $away; returned = $returned; controlProcessId = $PID; controlHwnd = $control.Handle.ToInt64() } |
      ConvertTo-Json -Compress -Depth 5
  }
  finally {
    if ($control) { $control.Close(); $control.Dispose() }
  }
  exit 0
}

$originRect = New-Object GraphTitlebarNativeInput+RECT
if (-not [GraphTitlebarNativeInput]::GetWindowRect($handle, [ref]$originRect)) {
  throw 'GetWindowRect failed before measurement.'
}
$width = $originRect.Right - $originRect.Left
$height = $originRect.Bottom - $originRect.Top
$OriginX = $originRect.Left
$OriginY = $originRect.Top

$titlebarX = $OriginX + [int]($width / 2)
$titlebarY = $OriginY + 16
$records = [System.Collections.Generic.List[object]]::new()
$mouseDown = $false
$restoreSucceeded = $false
$previousLeft = $OriginX
$previousTop = $OriginY
try {
  [GraphTitlebarNativeInput]::Mouse([GraphTitlebarNativeInput]::MOUSEEVENTF_MOVE, $titlebarX, $titlebarY)
  [GraphTitlebarNativeInput]::Mouse([GraphTitlebarNativeInput]::MOUSEEVENTF_LEFTDOWN, $titlebarX, $titlebarY)
  $mouseDown = $true
  for ($index = 1; $index -le $Samples; $index += 1) {
    $targetX = $titlebarX + ($index * $StepX)
    $targetY = $titlebarY + ($index * $StepY)
    [GraphTitlebarNativeInput]::Mouse([GraphTitlebarNativeInput]::MOUSEEVENTF_MOVE, $targetX, $targetY)
    Start-Sleep -Milliseconds $CadenceMs
    $rect = New-Object GraphTitlebarNativeInput+RECT
    $cursor = New-Object GraphTitlebarNativeInput+POINT
    if (-not [GraphTitlebarNativeInput]::GetWindowRect($handle, [ref]$rect)) { continue }
    [GraphTitlebarNativeInput]::GetCursorPos([ref]$cursor) | Out-Null
    $expectedLeft = $OriginX + ($index * $StepX)
    $expectedTop = $OriginY + ($index * $StepY)
    $errorX = $expectedLeft - $rect.Left
    $errorY = $expectedTop - $rect.Top
    $foreground = Get-ForegroundSnapshot
    $records.Add([pscustomobject]@{
      index = $index
      timestampUnixMs = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
      targetCursorX = $targetX
      targetCursorY = $targetY
      actualCursorX = $cursor.X
      actualCursorY = $cursor.Y
      expectedLeft = $expectedLeft
      expectedTop = $expectedTop
      actualLeft = $rect.Left
      actualTop = $rect.Top
      errorX = $errorX
      errorY = $errorY
      euclideanError = [Math]::Sqrt(($errorX * $errorX) + ($errorY * $errorY))
      movedSincePrevious = ($rect.Left -ne $previousLeft -or $rect.Top -ne $previousTop)
      foregroundHwnd = $foreground.hwnd
      foregroundProcessId = $foreground.processId
      currentProcessId = $ProcessId
      windowHwnd = $handle.ToInt64()
    })
    $previousLeft = $rect.Left
    $previousTop = $rect.Top
  }
}
finally {
  if ($mouseDown) {
    [GraphTitlebarNativeInput]::Mouse([GraphTitlebarNativeInput]::MOUSEEVENTF_LEFTUP, $titlebarX + ($Samples * $StepX), $titlebarY + ($Samples * $StepY))
  }
  Start-Sleep -Milliseconds 100
  $restoreSucceeded = [GraphTitlebarNativeInput]::MoveWindow($handle, $OriginX, $OriginY, $width, $height, $true)
}

[pscustomobject]@{
  stimulus = 'send-input-native-titlebar-drag'
  mode = 'send-input-native-titlebar-drag'
  positionObserver = 'GetWindowRect'
  processId = $ProcessId
  hwnd = $handle.ToInt64()
  origin = [pscustomobject]@{ left = $OriginX; top = $OriginY; width = $width; height = $height }
  dpi = [GraphTitlebarNativeInput]::GetDpiForWindow($handle)
  displayScalePercent = [Math]::Round(([double][GraphTitlebarNativeInput]::GetDpiForWindow($handle) / 96) * 100)
  sampleCount = $records.Count
  moveWindowUsedDuringMeasurement = $false
  mouseUpGuaranteedByFinally = $true
  restoredWithMoveWindowAfterMeasurement = $restoreSucceeded
  samples = $records
} | ConvertTo-Json -Compress -Depth 8
