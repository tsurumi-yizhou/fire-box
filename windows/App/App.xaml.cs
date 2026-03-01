using System;
using System.Diagnostics;
using System.IO;
using System.Runtime.InteropServices;
using System.Threading;
using Microsoft.UI.Xaml;
using App.Services;

namespace App;

public partial class App : Application
{
    // Single-instance mutex (process-scoped so it's released on exit)
    private static Mutex? _mutex;

    private Window? _window;
    private TrayIcon? _tray;

    public App()
    {
        InitializeComponent();
    }

    private void SetupService()
    {
        string exePath = Process.GetCurrentProcess().MainModule?.FileName ?? "";
        string dir = Path.GetDirectoryName(exePath) ?? "";
        string serviceBin = Path.Combine(dir, "firebox-service.exe");

        if (!File.Exists(serviceBin)) return;

        // Check if service exists
        using Process queryProc = new Process();
        queryProc.StartInfo.FileName = "sc.exe";
        queryProc.StartInfo.Arguments = "query firebox";
        queryProc.StartInfo.CreateNoWindow = true;
        queryProc.StartInfo.UseShellExecute = false;
        queryProc.Start();
        queryProc.WaitForExit();

        if (queryProc.ExitCode != 0) // Service not found
        {
            try
            {
                var psi = new ProcessStartInfo
                {
                    FileName = "sc.exe",
                    Arguments = $"create firebox binPath= \"{serviceBin}\" start= auto",
                    Verb = "runas",
                    UseShellExecute = true,
                    CreateNoWindow = true
                };
                Process.Start(psi)?.WaitForExit();
            }
            catch { /* User may have cancelled UAC */ }
        }

        // Try to start the service (will require admin if not running and we use runas)
        // Check if running first
        using Process stateProc = new Process();
        stateProc.StartInfo.FileName = "sc.exe";
        stateProc.StartInfo.Arguments = "query firebox";
        stateProc.StartInfo.CreateNoWindow = true;
        stateProc.StartInfo.UseShellExecute = false;
        stateProc.StartInfo.RedirectStandardOutput = true;
        stateProc.Start();
        string output = stateProc.StandardOutput.ReadToEnd();
        stateProc.WaitForExit();

        if (!output.Contains("RUNNING"))
        {
            try
            {
                var startPsi = new ProcessStartInfo
                {
                    FileName = "sc.exe",
                    Arguments = "start firebox",
                    Verb = "runas",
                    UseShellExecute = true,
                    CreateNoWindow = true
                };
                Process.Start(startPsi)?.WaitForExit();
            }
            catch { /* Ignore UAC cancellation */ }
        }
    }

    protected override void OnLaunched(LaunchActivatedEventArgs args)
    {
        // Single-instance guard
        _mutex = new Mutex(true, "Local\\FireBoxAppMutex", out bool isNewInstance);
        if (!isNewInstance)
        {
            Environment.Exit(0);
            return;
        }

        SetupService();

        _window = new MainWindow();

        // Hide to tray on close rather than destroying the window
        _window.AppWindow.Closing += (_, e) =>
        {
            e.Cancel = true;
            _window.AppWindow.Hide();
        };

        _window.Activate();
        _tray = new TrayIcon(_window);
    }
}

// ---------------------------------------------------------------------------
// Minimal system-tray icon using Shell_NotifyIcon P/Invoke
// ---------------------------------------------------------------------------

internal sealed class TrayIcon : IDisposable
{
    // Win32 constants
    private const int WM_APP    = 0x8000;
    private const int WM_TRAY   = WM_APP + 1;
    private const int WM_CLOSE  = 0x0010;
    private const int NIF_ICON  = 0x0002;
    private const int NIF_TIP   = 0x0004;
    private const int NIF_MSG   = 0x0001;
    private const int NIM_ADD    = 0x0000;
    private const int NIM_DELETE = 0x0002;
    private const int WM_LBUTTONUP   = 0x0202;
    private const int WM_RBUTTONUP   = 0x0205;
    private const int WM_CONTEXTMENU = 0x007B;
    private const int WM_COMMAND     = 0x0111;

    private const int MF_STRING      = 0x0000;
    private const int MF_SEPARATOR   = 0x0800;
    private const uint TPM_RIGHTBUTTON = 0x0002;
    private const uint TPM_RETURNCMD   = 0x0100;

    private const int IDM_OPEN = 1001;
    private const int IDM_QUIT = 1002;

    // Window class / window for the hidden message pump
    private const uint CS_HREDRAW   = 0x0002;
    private const uint CS_VREDRAW   = 0x0001;
    private const uint WS_OVERLAPPED = 0x00000000;

    private const int IDI_APPLICATION = 32512;

    [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
    private struct NOTIFYICONDATA
    {
        public uint cbSize;
        public IntPtr hWnd;
        public uint uID;
        public uint uFlags;
        public uint uCallbackMessage;
        public IntPtr hIcon;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 128)]
        public string szTip;
    }

    [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
    private struct WNDCLASSEX
    {
        public uint cbSize;
        public uint style;
        public WndProcDelegate lpfnWndProc;
        public int cbClsExtra;
        public int cbWndExtra;
        public IntPtr hInstance;
        public IntPtr hIcon;
        public IntPtr hCursor;
        public IntPtr hbrBackground;
        public string? lpszMenuName;
        public string lpszClassName;
        public IntPtr hIconSm;
    }

    private delegate IntPtr WndProcDelegate(IntPtr hWnd, uint msg, IntPtr wParam, IntPtr lParam);

    [DllImport("shell32.dll", CharSet = CharSet.Unicode)]
    private static extern bool Shell_NotifyIconW(uint dwMessage, ref NOTIFYICONDATA lpdata);

    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    private static extern ushort RegisterClassExW(ref WNDCLASSEX lpwcx);

    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    private static extern IntPtr CreateWindowExW(uint dwExStyle, string lpClassName,
        string lpWindowName, uint dwStyle, int X, int Y, int nWidth, int nHeight,
        IntPtr hWndParent, IntPtr hMenu, IntPtr hInstance, IntPtr lpParam);

    [DllImport("user32.dll")]
    private static extern IntPtr DefWindowProcW(IntPtr hWnd, uint msg, IntPtr wParam, IntPtr lParam);

    [DllImport("user32.dll")]
    private static extern bool DestroyWindow(IntPtr hWnd);

    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    private static extern bool UnregisterClassW(string lpClassName, IntPtr hInstance);

    [DllImport("user32.dll")]
    private static extern IntPtr LoadIconW(IntPtr hInstance, IntPtr lpIconName);

    [DllImport("kernel32.dll")]
    private static extern IntPtr GetModuleHandleW(IntPtr moduleName);

    [DllImport("user32.dll")]
    private static extern IntPtr CreatePopupMenu();

    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    private static extern bool InsertMenuW(IntPtr hMenu, uint uPosition, uint uFlags, nuint uIDNewItem, string lpNewItem);

    [DllImport("user32.dll")]
    private static extern bool DestroyMenu(IntPtr hMenu);

    [DllImport("user32.dll")]
    private static extern uint TrackPopupMenuEx(IntPtr hMenu, uint uFlags, int x, int y, IntPtr hWnd, IntPtr lptpm);

    [DllImport("user32.dll")]
    private static extern bool GetCursorPos(out POINT lpPoint);

    [DllImport("user32.dll")]
    private static extern bool SetForegroundWindow(IntPtr hWnd);

    [DllImport("user32.dll")]
    private static extern bool PostMessageW(IntPtr hWnd, uint msg, IntPtr wParam, IntPtr lParam);

    [StructLayout(LayoutKind.Sequential)]
    private struct POINT { public int X; public int Y; }

    private readonly Window _window;
    private readonly WndProcDelegate _wndProc;
    private readonly IntPtr _hWnd;
    private NOTIFYICONDATA _nid;
    private bool _disposed;

    private static uint _idCounter = 1;

    public TrayIcon(Window window)
    {
        _window = window;
        _wndProc = WndProc;  // keep delegate alive

        var hInstance = GetModuleHandleW(IntPtr.Zero);
        var className = "FireBoxTray_" + Environment.ProcessId;

        var wc = new WNDCLASSEX
        {
            cbSize        = (uint)Marshal.SizeOf<WNDCLASSEX>(),
            style         = CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc   = _wndProc,
            lpszClassName = className,
            hInstance     = hInstance,
        };
        RegisterClassExW(ref wc);

        _hWnd = CreateWindowExW(0, className, "FireBoxTrayMsg", WS_OVERLAPPED,
            0, 0, 0, 0, IntPtr.Zero, IntPtr.Zero, hInstance, IntPtr.Zero);

        var hIcon = LoadIconW(IntPtr.Zero, new IntPtr(IDI_APPLICATION));
        var id    = _idCounter++;

        _nid = new NOTIFYICONDATA
        {
            cbSize          = (uint)Marshal.SizeOf<NOTIFYICONDATA>(),
            hWnd            = _hWnd,
            uID             = id,
            uFlags          = NIF_ICON | NIF_TIP | NIF_MSG,
            uCallbackMessage= (uint)WM_TRAY,
            hIcon           = hIcon,
            szTip           = ResourceHelper.GetString("TrayIconTooltip"),
        };
        Shell_NotifyIconW(NIM_ADD, ref _nid);
    }

    private IntPtr WndProc(IntPtr hWnd, uint msg, IntPtr wParam, IntPtr lParam)
    {
        if (msg == WM_TRAY)
        {
            var mouseMsg = (int)lParam & 0xFFFF;
            if (mouseMsg == WM_LBUTTONUP)
                ShowMainWindow();
            else if (mouseMsg == WM_RBUTTONUP)
                ShowContextMenu(hWnd);
        }
        else if (msg == WM_COMMAND)
        {
            int id = (int)wParam & 0xFFFF;
            if (id == IDM_OPEN)
                ShowMainWindow();
            else if (id == IDM_QUIT)
                QuitApplication();
        }
        return DefWindowProcW(hWnd, msg, wParam, lParam);
    }

    private void ShowMainWindow()
    {
        _window.AppWindow.Show();
        _window.Activate();
    }

    private void ShowContextMenu(IntPtr hWnd)
    {
        IntPtr hMenu = CreatePopupMenu();
        InsertMenuW(hMenu, 0, MF_STRING, IDM_OPEN,
            ResourceHelper.GetString("TrayMenuOpen"));
        InsertMenuW(hMenu, 1, MF_SEPARATOR, 0, string.Empty);
        InsertMenuW(hMenu, 2, MF_STRING, IDM_QUIT,
            ResourceHelper.GetString("TrayMenuQuit"));

        GetCursorPos(out POINT pt);
        SetForegroundWindow(hWnd);
        TrackPopupMenuEx(hMenu, TPM_RIGHTBUTTON, pt.X, pt.Y, hWnd, IntPtr.Zero);
        PostMessageW(hWnd, WM_CLOSE, IntPtr.Zero, IntPtr.Zero);
        DestroyMenu(hMenu);
    }

    private void QuitApplication()
    {
        Dispose();
        Environment.Exit(0);
    }

    public void Dispose()
    {
        if (_disposed) return;
        _disposed = true;
        Shell_NotifyIconW(NIM_DELETE, ref _nid);
        if (_hWnd != IntPtr.Zero)
            DestroyWindow(_hWnd);
    }
}
