using System.IO;

namespace Chatstronomy.NINA.Configuration;

internal abstract record ChatDeliveryConfiguration;

internal sealed record DiscordWebhookDeliveryConfiguration(Uri WebhookUrl)
    : ChatDeliveryConfiguration;

internal sealed record DiscordBotDeliveryConfiguration(
    string BotToken,
    ulong ApplicationId,
    ulong DefaultChannelId)
    : ChatDeliveryConfiguration;

internal sealed record HostedDeliveryConfiguration(
    Uri ServiceUrl,
    string CredentialReference)
    : ChatDeliveryConfiguration;

internal sealed record MatrixDeliveryConfiguration(
    Uri HomeserverUrl,
    string Username,
    string Password,
    string DefaultRoomId);

internal sealed record LocalRuntimeConfiguration(
    string ExecutablePath,
    bool StartWithNina,
    bool StopWithNina);

internal sealed record ChatstronomyConfiguration(
    ChatDeliveryConfiguration Delivery,
    MatrixDeliveryConfiguration? Matrix,
    LocalRuntimeConfiguration? LocalRuntime);

internal static class ChatstronomyConfigurationValidator
{
    public static Uri RequireDiscordWebhook(string value)
    {
        if (!Uri.TryCreate(value, UriKind.Absolute, out var uri)
            || uri.Scheme != Uri.UriSchemeHttps
            || !IsDiscordHost(uri.Host)
            || !uri.AbsolutePath.StartsWith("/api/webhooks/", StringComparison.OrdinalIgnoreCase))
        {
            throw new InvalidOperationException(
                "Enter a valid HTTPS Discord webhook URL (…/api/webhooks/…).");
        }

        return uri;
    }

    public static ulong RequireDiscordSnowflake(string value, string label)
    {
        if (!ulong.TryParse(value, out var id) || id == 0)
        {
            throw new InvalidOperationException($"{label} must be a numeric Discord ID.");
        }

        return id;
    }

    public static string RequireSecret(string value, string label)
    {
        if (string.IsNullOrWhiteSpace(value))
        {
            throw new InvalidOperationException($"{label} is required.");
        }

        return value;
    }

    public static Uri RequireHostedUrl(string value)
    {
        if (!Uri.TryCreate(value, UriKind.Absolute, out var uri)
            || uri.Scheme != Uri.UriSchemeHttps)
        {
            throw new InvalidOperationException(
                "Chatstronomy service URL must be an absolute https:// URL.");
        }

        return uri;
    }

    public static Uri RequireMatrixHomeserver(string value)
    {
        if (!Uri.TryCreate(value, UriKind.Absolute, out var uri)
            || (uri.Scheme != Uri.UriSchemeHttps && uri.Scheme != Uri.UriSchemeHttp))
        {
            throw new InvalidOperationException(
                "Matrix homeserver URL must be an absolute http:// or https:// URL.");
        }

        return uri;
    }

    public static LocalRuntimeConfiguration BuildLocalRuntime(
        string executablePath,
        bool startWithNina,
        bool stopWithNina)
    {
        var path = executablePath?.Trim() ?? string.Empty;
        if (startWithNina && string.IsNullOrWhiteSpace(path))
        {
            throw new InvalidOperationException(
                "Select the Chatstronomy runtime executable or turn off automatic start.");
        }

        if (startWithNina && !File.Exists(path))
        {
            throw new InvalidOperationException(
                "The configured Chatstronomy runtime executable was not found.");
        }

        // Only a process started by this plugin is eligible for teardown. This
        // prevents "Stop with N.I.N.A." from terminating a separately managed
        // Chatstronomy instance.
        return new LocalRuntimeConfiguration(
            path,
            startWithNina,
            startWithNina && stopWithNina);
    }

    private static bool IsDiscordHost(string host) =>
        host.Equals("discord.com", StringComparison.OrdinalIgnoreCase)
        || host.EndsWith(".discord.com", StringComparison.OrdinalIgnoreCase)
        || host.Equals("discordapp.com", StringComparison.OrdinalIgnoreCase)
        || host.EndsWith(".discordapp.com", StringComparison.OrdinalIgnoreCase);
}
