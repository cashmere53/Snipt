# OS-side helpers for the E2E tests: real key input, the foreground window, closing a window
# like its close button, and the clipboard image. Runs as one long-lived process (STA, for the
# clipboard): reads one JSON command per line on stdin and writes one JSON result per line.
$ErrorActionPreference = 'Stop'
# Window titles are Japanese: the pipes must be UTF-8 both ways.
[Console]::InputEncoding = [System.Text.UTF8Encoding]::new($false)
[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false)
Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing
Add-Type @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;

public static class SniptE2e {
    delegate bool EnumProc(IntPtr hwnd, IntPtr lParam);
    [DllImport("user32.dll")] static extern bool EnumWindows(EnumProc proc, IntPtr lParam);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] static extern int GetWindowText(IntPtr hwnd, StringBuilder text, int max);
    [DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr hwnd);
    [DllImport("user32.dll")] static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")] static extern bool SetForegroundWindow(IntPtr hwnd);
    [DllImport("user32.dll")] static extern bool PostMessage(IntPtr hwnd, uint msg, IntPtr w, IntPtr l);
    [DllImport("user32.dll")] static extern void keybd_event(byte vk, byte scan, uint flags, UIntPtr extra);
    [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr hwnd, IntPtr pid);
    [DllImport("user32.dll")] static extern bool AttachThreadInput(uint attach, uint to, bool on);
    [DllImport("user32.dll")] static extern bool BringWindowToTop(IntPtr hwnd);
    [DllImport("kernel32.dll")] static extern uint GetCurrentThreadId();

    const uint KEYEVENTF_KEYUP = 2;
    const uint WM_CLOSE = 0x10;

    public static string Title(IntPtr hwnd) {
        var text = new StringBuilder(512);
        GetWindowText(hwnd, text, text.Capacity);
        return text.ToString();
    }

    public static List<IntPtr> Find(string title) {
        var found = new List<IntPtr>();
        EnumWindows((hwnd, _) => {
            if (IsWindowVisible(hwnd) && Title(hwnd) == title) { found.Add(hwnd); }
            return true;
        }, IntPtr.Zero);
        return found;
    }

    public static string Foreground() { return Title(GetForegroundWindow()); }

    public static bool Activate(string title) {
        var windows = Find(title);
        if (windows.Count == 0) { return false; }
        // Sharing the foreground thread's input state lets this process move the foreground
        // (the foreground lock). The usual tapped Alt is avoided: its key-up puts the newly
        // activated window into menu mode, which stalls the app's UI thread and WebDriver.
        var me = GetCurrentThreadId();
        var foreground = GetWindowThreadProcessId(GetForegroundWindow(), IntPtr.Zero);
        var attached = foreground != 0 && foreground != me && AttachThreadInput(me, foreground, true);
        try {
            BringWindowToTop(windows[0]);
            return SetForegroundWindow(windows[0]);
        } finally {
            if (attached) { AttachThreadInput(me, foreground, false); }
        }
    }

    public static bool Close(string title) {
        var windows = Find(title);
        foreach (var hwnd in windows) { PostMessage(hwnd, WM_CLOSE, IntPtr.Zero, IntPtr.Zero); }
        return windows.Count > 0;
    }

    public static void Keys(byte[] vks) {
        foreach (var vk in vks) { keybd_event(vk, 0, 0, UIntPtr.Zero); }
        Array.Reverse(vks);
        foreach (var vk in vks) { keybd_event(vk, 0, KEYEVENTF_KEYUP, UIntPtr.Zero); }
    }
}
'@

function Read-ClipboardImage($points) {
    $png = [System.Windows.Forms.Clipboard]::GetData('PNG')
    if ($png -is [System.IO.Stream]) {
        $image = [System.Drawing.Bitmap]::new($png)
    } else {
        $image = [System.Windows.Forms.Clipboard]::GetImage()
    }
    if ($null -eq $image) { return $null }
    $bitmap = [System.Drawing.Bitmap]::new($image)
    # Flat `r,g,b,a,...`: single nested arrays would be unrolled on the way out too.
    $pixels = [System.Collections.Generic.List[int]]::new()
    foreach ($p in ($points -split ';' | Where-Object { $_ })) {
        $xy = $p -split ','
        $c = $bitmap.GetPixel([int]$xy[0], [int]$xy[1])
        $pixels.AddRange([int[]]@($c.R, $c.G, $c.B, $c.A))
    }
    $pixels = $pixels.ToArray()
    $result = @{ width = $bitmap.Width; height = $bitmap.Height; pixels = $pixels }
    $bitmap.Dispose()
    $image.Dispose()
    return $result
}

while ($null -ne ($line = [Console]::In.ReadLine())) {
    try {
        $cmd = $line | ConvertFrom-Json
        $value = switch ($cmd.op) {
            'keys' { [SniptE2e]::Keys([byte[]]$cmd.vks); $true }
            'foreground' { [SniptE2e]::Foreground() }
            'activate' { [SniptE2e]::Activate($cmd.title) }
            'close' { [SniptE2e]::Close($cmd.title) }
            'exists' { [SniptE2e]::Find($cmd.title).Count -gt 0 }
            'clipboardClear' { [System.Windows.Forms.Clipboard]::Clear(); $true }
            'clipboardImage' { Read-ClipboardImage $cmd.points }
            default { throw "unknown op: $($cmd.op)" }
        }
        [Console]::Out.WriteLine((@{ ok = $true; value = $value } | ConvertTo-Json -Compress -Depth 6))
    } catch {
        [Console]::Out.WriteLine((@{ ok = $false; error = "$_" } | ConvertTo-Json -Compress))
    }
}
