using System.IO;
using System.Reflection;
using NINA.Plugin;
using NINA.Profile;
using NINA.Profile.Interfaces;

namespace Chatstronomy.NINA.Settings;

internal sealed class ChatstronomySettings
{
    internal static readonly Guid PluginId =
        Guid.Parse("5e7c25c4-f654-4e22-9e21-3127048221c0");

    private const string CredentialPrefix = "Chatstronomy.NINA";
    private readonly IProfileService profileService;
    private readonly PluginOptionsAccessor options;

    public ChatstronomySettings(IProfileService profileService)
    {
        this.profileService = profileService;
        options = new PluginOptionsAccessor(profileService, PluginId);
    }

    public ChatDeliveryMode DeliveryMode
    {
        get
        {
            var value = options.GetValueString(
                nameof(DeliveryMode),
                ChatDeliveryMode.DiscordWebhook.ToString());
            return Enum.TryParse<ChatDeliveryMode>(value, out var mode)
                ? mode
                : ChatDeliveryMode.DiscordWebhook;
        }
        set => options.SetValueString(nameof(DeliveryMode), value.ToString());
    }

    public string DiscordWebhookUrl
    {
        get => WindowsCredentialStore.Read(CredentialTarget("discord-webhook")) ?? string.Empty;
        set => WindowsCredentialStore.Write(CredentialTarget("discord-webhook"), value?.Trim());
    }

    public string DiscordBotToken
    {
        get => WindowsCredentialStore.Read(CredentialTarget("discord-bot-token")) ?? string.Empty;
        set => WindowsCredentialStore.Write(CredentialTarget("discord-bot-token"), value?.Trim());
    }

    public string DiscordApplicationId
    {
        get => options.GetValueString(nameof(DiscordApplicationId), string.Empty);
        set => options.SetValueString(nameof(DiscordApplicationId), value?.Trim() ?? string.Empty);
    }

    public string DiscordChannelId
    {
        get => options.GetValueString(nameof(DiscordChannelId), string.Empty);
        set => options.SetValueString(nameof(DiscordChannelId), value?.Trim() ?? string.Empty);
    }

    public string HostedServiceUrl
    {
        get => options.GetValueString(nameof(HostedServiceUrl), "https://chatstronomy.com/");
        set => options.SetValueString(nameof(HostedServiceUrl), value?.Trim() ?? string.Empty);
    }

    /// <summary>
    /// An opaque reference issued by the hosted credential flow. The actual
    /// credential is resolved by that flow and is never stored in profile JSON.
    /// </summary>
    public string HostedCredentialReference
    {
        get => options.GetValueString(nameof(HostedCredentialReference), string.Empty);
        set => options.SetValueString(
            nameof(HostedCredentialReference),
            value?.Trim() ?? string.Empty);
    }

    public string LocalRuntimePath
    {
        get => options.GetValueString(nameof(LocalRuntimePath), DefaultRuntimePath());
        set => options.SetValueString(nameof(LocalRuntimePath), value?.Trim() ?? string.Empty);
    }

    public bool StartLocalRuntime
    {
        get => options.GetValueBoolean(nameof(StartLocalRuntime), true);
        set => options.SetValueBoolean(nameof(StartLocalRuntime), value);
    }

    public bool StopLocalRuntimeWithNina
    {
        get => options.GetValueBoolean(nameof(StopLocalRuntimeWithNina), true);
        set => options.SetValueBoolean(nameof(StopLocalRuntimeWithNina), value);
    }

    private string CredentialTarget(string kind) =>
        $"{CredentialPrefix}/{profileService.ActiveProfile.Id:D}/{kind}";

    private static string DefaultRuntimePath()
    {
        var assemblyDirectory = Path.GetDirectoryName(
            Assembly.GetExecutingAssembly().Location) ?? AppContext.BaseDirectory;
        return Path.Combine(assemblyDirectory, "runtime", "chatstronomy.exe");
    }
}
