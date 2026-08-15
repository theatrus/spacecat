using System.ComponentModel;
using System.ComponentModel.Composition;
using System.Runtime.CompilerServices;
using NINA.Plugin;
using NINA.Plugin.Interfaces;
using NINA.Profile.Interfaces;
using Chatstronomy.NINA.Configuration;
using Chatstronomy.NINA.Protocol;
using Chatstronomy.NINA.Settings;

namespace Chatstronomy.NINA;

/// <summary>
/// N.I.N.A. lifecycle entry point for Chatstronomy.
///
/// Direct event subscriptions and local/remote transports will be added behind
/// this manifest in follow-up changes. Keeping the manifest in the main Chatstronomy
/// repository allows the native plugin and Rust protocol to be released and
/// tested together.
/// </summary>
[Export(typeof(IPluginManifest))]
public sealed class ChatstronomyPlugin : PluginBase, INotifyPropertyChanged
{
    private readonly IProfileService profileService;
    private readonly ChatstronomySettings settings;
    private readonly Guid nodeId = NodeIdentityStore.LoadOrCreate();
    private readonly Guid sessionId = Guid.NewGuid();
    private DirectConnectionSettings connectionSettings = DirectConnectionSettings.Local;

    [ImportingConstructor]
    public ChatstronomyPlugin(IProfileService profileService)
    {
        this.profileService = profileService;
        settings = new ChatstronomySettings(profileService);
    }

    public event PropertyChangedEventHandler? PropertyChanged;

    public bool UseDiscordWebhook
    {
        get => settings.DeliveryMode == ChatDeliveryMode.DiscordWebhook;
        set
        {
            if (value)
            {
                SetDeliveryMode(ChatDeliveryMode.DiscordWebhook);
            }
        }
    }

    public bool UseDiscordBot
    {
        get => settings.DeliveryMode == ChatDeliveryMode.DiscordBot;
        set
        {
            if (value)
            {
                SetDeliveryMode(ChatDeliveryMode.DiscordBot);
            }
        }
    }

    public bool UseHostedService
    {
        get => settings.DeliveryMode == ChatDeliveryMode.HostedService;
        set
        {
            if (value)
            {
                SetDeliveryMode(ChatDeliveryMode.HostedService);
            }
        }
    }

    public bool UsesLocalRuntime => !UseHostedService;

    public string DiscordWebhookUrl
    {
        get => settings.DiscordWebhookUrl;
        set
        {
            settings.DiscordWebhookUrl = value;
            RefreshStatus();
        }
    }

    public string DiscordBotToken
    {
        get => settings.DiscordBotToken;
        set
        {
            settings.DiscordBotToken = value;
            RefreshStatus();
        }
    }

    public string DiscordApplicationId
    {
        get => settings.DiscordApplicationId;
        set
        {
            settings.DiscordApplicationId = value;
            RaisePropertyChanged();
            RefreshStatus();
        }
    }

    public string DiscordChannelId
    {
        get => settings.DiscordChannelId;
        set
        {
            settings.DiscordChannelId = value;
            RaisePropertyChanged();
            RefreshStatus();
        }
    }

    public string HostedServiceUrl
    {
        get => settings.HostedServiceUrl;
        set
        {
            settings.HostedServiceUrl = value;
            RaisePropertyChanged();
            RefreshStatus();
        }
    }

    /// <summary>
    /// Written by the hosted credential flow. It is deliberately opaque here;
    /// this plugin configuration never serializes the hosted secret itself.
    /// </summary>
    public string HostedCredentialReference
    {
        get => settings.HostedCredentialReference;
        set
        {
            settings.HostedCredentialReference = value;
            RaisePropertyChanged();
            RaisePropertyChanged(nameof(HostedCredentialStatus));
            RefreshStatus();
        }
    }

    public string HostedCredentialStatus =>
        string.IsNullOrWhiteSpace(HostedCredentialReference)
            ? "Not connected. Complete the Chatstronomy.com sign-in or pairing flow."
            : "A hosted credential is available for this N.I.N.A. profile.";

    public string LocalRuntimePath
    {
        get => settings.LocalRuntimePath;
        set
        {
            settings.LocalRuntimePath = value;
            RaisePropertyChanged();
            RefreshStatus();
        }
    }

    public bool StartLocalRuntime
    {
        get => settings.StartLocalRuntime;
        set
        {
            settings.StartLocalRuntime = value;
            RaisePropertyChanged();
            RefreshStatus();
        }
    }

    public bool StopLocalRuntimeWithNina
    {
        get => settings.StopLocalRuntimeWithNina;
        set
        {
            settings.StopLocalRuntimeWithNina = value;
            RaisePropertyChanged();
            RefreshStatus();
        }
    }

    public bool IsConfigurationValid
    {
        get
        {
            try
            {
                _ = BuildConfiguration();
                return true;
            }
            catch (InvalidOperationException)
            {
                return false;
            }
        }
    }

    public string ConfigurationStatus
    {
        get
        {
            try
            {
                _ = BuildConfiguration();
                return UseHostedService
                    ? "Ready to connect to Chatstronomy.com."
                    : StartLocalRuntime
                        ? "Configuration is ready to start the local Chatstronomy runtime."
                        : "Configuration is ready; Chatstronomy must already be running locally.";
            }
            catch (InvalidOperationException exception)
            {
                return exception.Message;
            }
        }
    }

    public override Task Initialize()
    {
        profileService.ProfileChanged += ProfileServiceProfileChanged;
        return base.Initialize();
    }

    public override Task Teardown()
    {
        profileService.ProfileChanged -= ProfileServiceProfileChanged;
        return base.Teardown();
    }

    internal ChatstronomyConfiguration BuildConfiguration()
    {
        ChatDeliveryConfiguration delivery = settings.DeliveryMode switch
        {
            ChatDeliveryMode.DiscordWebhook => new DiscordWebhookDeliveryConfiguration(
                ChatstronomyConfigurationValidator.RequireDiscordWebhook(DiscordWebhookUrl)),
            ChatDeliveryMode.DiscordBot => new DiscordBotDeliveryConfiguration(
                ChatstronomyConfigurationValidator.RequireSecret(
                    DiscordBotToken,
                    "Discord bot token"),
                ChatstronomyConfigurationValidator.RequireDiscordSnowflake(
                    DiscordApplicationId,
                    "Discord application ID"),
                ChatstronomyConfigurationValidator.RequireDiscordSnowflake(
                    DiscordChannelId,
                    "Default Discord channel ID")),
            ChatDeliveryMode.HostedService => new HostedDeliveryConfiguration(
                ChatstronomyConfigurationValidator.RequireHostedUrl(HostedServiceUrl),
                ChatstronomyConfigurationValidator.RequireSecret(
                    HostedCredentialReference,
                    "Hosted credential")),
            _ => throw new InvalidOperationException("Unknown Chatstronomy delivery mode."),
        };

        var localRuntime = UsesLocalRuntime
            ? ChatstronomyConfigurationValidator.BuildLocalRuntime(
                LocalRuntimePath,
                StartLocalRuntime,
                StopLocalRuntimeWithNina)
            : null;
        return new ChatstronomyConfiguration(delivery, localRuntime);
    }

    internal DirectConnectionSettings ConnectionSettings
    {
        get => connectionSettings;
        set
        {
            value.Validate();
            connectionSettings = value;
        }
    }

    /// <summary>
    /// Build the identity handshake used when this N.I.N.A. process connects
    /// to a local or remote Chatstronomy hub. The node and profile GUIDs are
    /// stable; the session GUID changes each time the plugin is loaded.
    /// </summary>
    internal ClientHello CreateClientHello()
    {
        var activeProfile = profileService.ActiveProfile;
        var pluginVersion = typeof(ChatstronomyPlugin).Assembly.GetName().Version?.ToString() ?? "unknown";
        var ninaVersion = typeof(IProfileService).Assembly.GetName().Version?.ToString() ?? "unknown";

        return new ClientHello(
            ProtocolVersion: DirectProtocol.CurrentVersion,
            NodeId: nodeId,
            SessionId: sessionId,
            ProcessId: Environment.ProcessId,
            ProfileId: activeProfile.Id,
            ProfileName: activeProfile.Name,
            PluginVersion: pluginVersion,
            NinaVersion: ninaVersion,
            Capabilities: DirectCapabilities.None);
    }

    private void SetDeliveryMode(ChatDeliveryMode mode)
    {
        if (settings.DeliveryMode == mode)
        {
            return;
        }

        settings.DeliveryMode = mode;
        RefreshAllProperties();
    }

    private void ProfileServiceProfileChanged(object? sender, EventArgs args) =>
        RefreshAllProperties();

    private void RefreshAllProperties()
    {
        foreach (var propertyName in new[]
        {
            nameof(UseDiscordWebhook),
            nameof(UseDiscordBot),
            nameof(UseHostedService),
            nameof(UsesLocalRuntime),
            nameof(DiscordWebhookUrl),
            nameof(DiscordBotToken),
            nameof(DiscordApplicationId),
            nameof(DiscordChannelId),
            nameof(HostedServiceUrl),
            nameof(HostedCredentialReference),
            nameof(HostedCredentialStatus),
            nameof(LocalRuntimePath),
            nameof(StartLocalRuntime),
            nameof(StopLocalRuntimeWithNina),
            nameof(IsConfigurationValid),
            nameof(ConfigurationStatus),
        })
        {
            RaisePropertyChanged(propertyName);
        }
    }

    private void RefreshStatus()
    {
        RaisePropertyChanged(nameof(IsConfigurationValid));
        RaisePropertyChanged(nameof(ConfigurationStatus));
    }

    private void RaisePropertyChanged([CallerMemberName] string? propertyName = null) =>
        PropertyChanged?.Invoke(this, new PropertyChangedEventArgs(propertyName));
}
