using System;
using System.Globalization;
using System.Reflection;
using System.Runtime.InteropServices;
using System.Resources;

internal static partial class Program
{
    private const int ExitApproved = 0;
    private const int ExitDenied = 1;
    private const int ExitError = 2;

    private const int TDCBF_OK_BUTTON = 0x0001;
    private const int TDCBF_CANCEL_BUTTON = 0x0008;
    private const int IDOK = 1;
    private static readonly IntPtr DpiAwarenessContextPerMonitorAwareV2 = new(-4);
    private static readonly ResourceManager Resources = new("Helper.Strings", Assembly.GetExecutingAssembly());

    [STAThread]
    private static int Main(string[] args)
    {
        SetProcessDpiAwarenessContext(DpiAwarenessContextPerMonitorAwareV2);

        var requesterName = GetRequesterName(args);
        var title = GetString("DialogTitle", "AI Capability Request");
        var instructionTemplate = GetString("MainInstructionFormat", "{0} wants to use AI capabilities. Approve?");
        var instruction = string.Format(CultureInfo.CurrentUICulture, instructionTemplate, requesterName);
        var content = GetString("Content", "This request is sent by the local AI capability management service.");

        var hr = TaskDialog(
            IntPtr.Zero,
            IntPtr.Zero,
            title,
            instruction,
            content,
            TDCBF_OK_BUTTON | TDCBF_CANCEL_BUTTON,
            IntPtr.Zero,
            out var pressedButton);

        if (hr < 0)
        {
            return ExitError;
        }

        return pressedButton == IDOK ? ExitApproved : ExitDenied;
    }

    private static string GetRequesterName(string[] args)
    {
        if (args.Length > 0)
        {
            var positional = args[0].Trim();
            if (positional.Length > 0)
            {
                return positional;
            }
        }

        return GetString("DefaultRequesterName", "An application");
    }

    private static string GetString(string key, string fallback)
    {
        return Resources.GetString(key, CultureInfo.CurrentUICulture) ?? fallback;
    }

    [DllImport("comctl32.dll", CharSet = CharSet.Unicode, ExactSpelling = true)]
    private static extern int TaskDialog(
        IntPtr hwndParent,
        IntPtr hInstance,
        string windowTitle,
        string mainInstruction,
        string content,
        int commonButtons,
        IntPtr icon,
        out int button);

    [DllImport("user32.dll", ExactSpelling = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool SetProcessDpiAwarenessContext(IntPtr value);
}
