namespace Chatstronomy.NINA.Settings;

/// <summary>
/// How the Direct plugin reaches Chatstronomy. Local is deliberately the default;
/// Remote must be selected and configured explicitly.
/// </summary>
internal enum DirectConnectionMode
{
    Local,
    Remote,
}

internal sealed record DirectConnectionSettings(
    DirectConnectionMode Mode,
    string? HubUrl)
{
    internal static DirectConnectionSettings Local { get; } = new(
        Mode: DirectConnectionMode.Local,
        HubUrl: null);

    internal static DirectConnectionSettings Remote(string hubUrl)
    {
        var settings = new DirectConnectionSettings(
            Mode: DirectConnectionMode.Remote,
            HubUrl: hubUrl);
        settings.Validate();
        return settings;
    }

    internal void Validate()
    {
        if (Mode == DirectConnectionMode.Local)
        {
            if (!string.IsNullOrWhiteSpace(HubUrl))
            {
                throw new InvalidOperationException("Local mode does not accept a hub URL");
            }
            return;
        }

        if (!Uri.TryCreate(HubUrl, UriKind.Absolute, out var uri)
            || !string.Equals(uri.Scheme, "wss", StringComparison.OrdinalIgnoreCase))
        {
            throw new InvalidOperationException("Remote mode requires an absolute wss:// hub URL");
        }
    }
}
