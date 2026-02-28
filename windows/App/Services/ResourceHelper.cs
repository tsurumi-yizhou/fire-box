using System;
using Windows.ApplicationModel.Resources;

namespace App.Services;

/// <summary>
/// Helper class for accessing localized resources.
/// </summary>
public static class ResourceHelper
{
    private static readonly ResourceLoader _loader = new();

    /// <summary>
    /// Gets a localized string by key.
    /// </summary>
    public static string GetString(string key)
    {
        return _loader.GetString(key);
    }

    /// <summary>
    /// Gets a formatted localized string with arguments.
    /// </summary>
    public static string GetString(string key, params object[] args)
    {
        var format = _loader.GetString(key);
        return string.Format(format, args);
    }
}
