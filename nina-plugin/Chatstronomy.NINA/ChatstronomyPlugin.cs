using System.ComponentModel;
using System.ComponentModel.Composition;
using System.Runtime.CompilerServices;
using System.Windows.Input;
using NINA.Plugin;
using NINA.Plugin.Interfaces;
using NINA.Profile.Interfaces;
using NINA.Equipment.Interfaces.Mediator;
using NINA.Sequencer.Interfaces.Mediator;
using NINA.Core.Utility.WindowService;
using NINA.WPF.Base.Interfaces;
using NINA.WPF.Base.Interfaces.Mediator;
using NINA.WPF.Base.Interfaces.ViewModel;
using Chatstronomy.NINA.Configuration;
using Chatstronomy.NINA.Direct;
using Chatstronomy.NINA.Protocol;
using Chatstronomy.NINA.Runtime;
using Chatstronomy.NINA.Settings;
using Chatstronomy.NINA.UI;

namespace Chatstronomy.NINA;

/// <summary>
/// N.I.N.A. lifecycle entry point for Chatstronomy.
///
/// The local Direct provider and supervised runtime live behind this manifest;
/// the remote transport can reuse the same protocol and native data provider.
/// Keeping the manifest in the main Chatstronomy repository allows the C# and
/// Rust sides to be released and tested together.
/// </summary>
[Export(typeof(IPluginManifest))]
public sealed class ChatstronomyPlugin : PluginBase, INotifyPropertyChanged
{
    private readonly IProfileService profileService;
    private readonly ChatstronomySettings settings;
    private readonly IChatstronomyRuntimeController runtimeController;
    private readonly INinaDirectDataProvider directDataProvider;
    private readonly AsyncCommand startRuntimeCommand;
    private readonly AsyncCommand stopRuntimeCommand;
    private readonly Guid nodeId = NodeIdentityStore.LoadOrCreate();
    private readonly Guid sessionId = Guid.NewGuid();
    private DirectConnectionSettings connectionSettings = DirectConnectionSettings.Local;

    [ImportingConstructor]
    public ChatstronomyPlugin(
        IProfileService profileService,
        ITelescopeMediator telescope,
        ICameraMediator camera,
        IFilterWheelMediator filterWheel,
        IGuiderMediator guider,
        IRotatorMediator rotator,
        IFocuserMediator focuser,
        ISequenceMediator sequence,
        IImageSaveMediator imageSave,
        IApplicationStatusMediator applicationStatus,
        IAutoFocusVMFactory autoFocusFactory,
        IImageHistoryVM imageHistory,
        IWindowServiceFactory windowFactory)
    {
        this.profileService = profileService;
        settings = new ChatstronomySettings(profileService);
        directDataProvider = new NinaDirectDataProvider(
            profileService,
            telescope,
            camera,
            filterWheel,
            guider,
            rotator,
            focuser,
            sequence,
            imageSave,
            applicationStatus,
            autoFocusFactory,
            imageHistory,
            windowFactory);
        runtimeController = new ChatstronomyRuntimeController(directDataProvider);
        startRuntimeCommand = new AsyncCommand(
            RestartLocalRuntimeAsync,
            () => UsesLocalRuntime && IsConfigurationValid);
        stopRuntimeCommand = new AsyncCommand(
            StopLocalRuntimeAsync,
            () => runtimeController.IsRunning);
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

    public bool UseMatrixOnly
    {
        get => settings.DeliveryMode == ChatDeliveryMode.MatrixOnly;
        set
        {
            if (value)
            {
                SetDeliveryMode(ChatDeliveryMode.MatrixOnly);
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

    public bool CanToggleLocalMatrix => UsesLocalRuntime && !UseMatrixOnly;

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

    public bool UseLocalMatrix
    {
        get => UseMatrixOnly || settings.UseLocalMatrix;
        set
        {
            settings.UseLocalMatrix = value;
            RaisePropertyChanged();
            RefreshStatus();
        }
    }

    public string MatrixHomeserverUrl
    {
        get => settings.MatrixHomeserverUrl;
        set
        {
            settings.MatrixHomeserverUrl = value;
            RaisePropertyChanged();
            RefreshStatus();
        }
    }

    public string MatrixUsername
    {
        get => settings.MatrixUsername;
        set
        {
            settings.MatrixUsername = value;
            RaisePropertyChanged();
            RefreshStatus();
        }
    }

    public string MatrixPassword
    {
        get => settings.MatrixPassword;
        set
        {
            settings.MatrixPassword = value;
            RefreshStatus();
        }
    }

    public string MatrixRoomId
    {
        get => settings.MatrixRoomId;
        set
        {
            settings.MatrixRoomId = value;
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

    public string AdvancedApiBaseUrl
    {
        get => settings.AdvancedApiBaseUrl;
        set
        {
            settings.AdvancedApiBaseUrl = value;
            RaisePropertyChanged();
            RefreshStatus();
        }
    }

    public bool UseDirectSource
    {
        get => settings.RuntimeSourceMode == RuntimeSourceMode.Direct;
        set
        {
            if (value)
            {
                SetRuntimeSourceMode(RuntimeSourceMode.Direct);
            }
        }
    }

    public bool UseAdvancedApiSource
    {
        get => settings.RuntimeSourceMode == RuntimeSourceMode.AdvancedApi;
        set
        {
            if (value)
            {
                SetRuntimeSourceMode(RuntimeSourceMode.AdvancedApi);
            }
        }
    }

    public string PollingIntervalSeconds
    {
        get => settings.PollingIntervalSeconds;
        set
        {
            settings.PollingIntervalSeconds = value;
            RaisePropertyChanged();
            RefreshStatus();
        }
    }

    public bool StartLocalRuntime
    {
        get => UseDirectSource || settings.StartLocalRuntime;
        set
        {
            if (UseAdvancedApiSource)
            {
                settings.StartLocalRuntime = value;
            }
            RaisePropertyChanged();
            RefreshStatus();
        }
    }

    public bool StopLocalRuntimeWithNina
    {
        get => UseDirectSource || settings.StopLocalRuntimeWithNina;
        set
        {
            if (UseAdvancedApiSource)
            {
                settings.StopLocalRuntimeWithNina = value;
            }
            RaisePropertyChanged();
            RefreshStatus();
        }
    }

    public ICommand StartRuntimeCommand => startRuntimeCommand;

    public ICommand StopRuntimeCommand => stopRuntimeCommand;

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
                    : runtimeController.IsRunning
                        ? runtimeController.StatusMessage
                        : StartLocalRuntime
                            ? $"Configuration is ready. {runtimeController.StatusMessage}"
                            : "Configuration is ready; Chatstronomy must already be running locally.";
            }
            catch (InvalidOperationException exception)
            {
                return exception.Message;
            }
        }
    }

    public override async Task Initialize()
    {
        directDataProvider.Start();
        profileService.ProfileChanged += ProfileServiceProfileChanged;
        runtimeController.StateChanged += RuntimeControllerStateChanged;
        await base.Initialize();
        await StartConfiguredRuntimeAsync(CancellationToken.None);
    }

    public override async Task Teardown()
    {
        profileService.ProfileChanged -= ProfileServiceProfileChanged;
        runtimeController.StateChanged -= RuntimeControllerStateChanged;
        if (runtimeController.IsRunning)
        {
            if (StopLocalRuntimeWithNina)
            {
                await runtimeController.StopAsync(CancellationToken.None);
            }
            else
            {
                await runtimeController.DetachAsync(CancellationToken.None);
            }
        }
        directDataProvider.Stop();
        await base.Teardown();
    }

    internal ChatstronomyConfiguration BuildConfiguration()
    {
        ChatDeliveryConfiguration? delivery = settings.DeliveryMode switch
        {
            ChatDeliveryMode.DiscordWebhook => new DiscordWebhookDeliveryConfiguration(
                ChatstronomyConfigurationValidator.RequireDiscordWebhook(DiscordWebhookUrl)),
            ChatDeliveryMode.DiscordBot => new DiscordBotDeliveryConfiguration(
                ChatstronomyConfigurationValidator.RequireSecret(
                    DiscordBotToken,
                    "Discord bot token"),
                ChatstronomyConfigurationValidator.OptionalDiscordSnowflake(
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
            ChatDeliveryMode.MatrixOnly => null,
            _ => throw new InvalidOperationException("Unknown Chatstronomy delivery mode."),
        };

        var localRuntime = UsesLocalRuntime
            ? ChatstronomyConfigurationValidator.BuildLocalRuntime(
                LocalRuntimePath,
                settings.RuntimeSourceMode,
                AdvancedApiBaseUrl,
                PollingIntervalSeconds,
                StartLocalRuntime,
                StopLocalRuntimeWithNina)
            : null;
        var matrix = UsesLocalRuntime && UseLocalMatrix
            ? new MatrixDeliveryConfiguration(
                ChatstronomyConfigurationValidator.RequireMatrixHomeserver(
                    MatrixHomeserverUrl),
                ChatstronomyConfigurationValidator.RequireSecret(
                    MatrixUsername,
                    "Matrix username"),
                ChatstronomyConfigurationValidator.RequireSecret(
                    MatrixPassword,
                    "Matrix password"),
                ChatstronomyConfigurationValidator.RequireSecret(
                    MatrixRoomId,
                    "Default Matrix room ID"))
            : null;
        return new ChatstronomyConfiguration(delivery, matrix, localRuntime);
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
            Capabilities: directDataProvider.Capabilities);
    }

    private void SetRuntimeSourceMode(RuntimeSourceMode mode)
    {
        if (settings.RuntimeSourceMode == mode)
        {
            return;
        }

        settings.RuntimeSourceMode = mode;
        RefreshAllProperties();
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

    private async void ProfileServiceProfileChanged(object? sender, EventArgs args)
    {
        try
        {
            if (runtimeController.IsRunning)
            {
                await runtimeController.StopAsync(CancellationToken.None);
            }
            directDataProvider.Reset();
            RefreshAllProperties();
            await StartConfiguredRuntimeAsync(CancellationToken.None);
        }
        catch
        {
            RefreshStatus();
        }
    }

    private async Task StartConfiguredRuntimeAsync(CancellationToken cancellationToken)
    {
        if (!UsesLocalRuntime || !StartLocalRuntime)
        {
            RefreshStatus();
            return;
        }

        try
        {
            var configuration = BuildConfiguration();
            var profile = profileService.ActiveProfile;
            await runtimeController.StartAsync(
                configuration,
                new LocalRuntimeIdentity(nodeId, profile.Id, profile.Name),
                cancellationToken);
        }
        catch (InvalidOperationException)
        {
            // Incomplete settings are expected while the user is configuring
            // the plugin. ConfigurationStatus displays the validation error.
        }
        catch
        {
            // Runtime controller failures are retained in StatusMessage and
            // surfaced through ConfigurationStatus without failing N.I.N.A.
        }
        RefreshStatus();
    }

    private async Task RestartLocalRuntimeAsync()
    {
        if (runtimeController.IsRunning)
        {
            await runtimeController.StopAsync(CancellationToken.None);
        }

        try
        {
            var configuration = BuildConfiguration();
            var profile = profileService.ActiveProfile;
            await runtimeController.StartAsync(
                configuration,
                new LocalRuntimeIdentity(nodeId, profile.Id, profile.Name),
                CancellationToken.None);
        }
        catch
        {
            // RuntimeController retains a user-facing status message.
        }
        RefreshStatus();
    }

    private async Task StopLocalRuntimeAsync()
    {
        try
        {
            await runtimeController.StopAsync(CancellationToken.None);
        }
        catch
        {
            // RuntimeController retains a user-facing status message when the
            // controlled process cannot be stopped cleanly.
        }
        RefreshStatus();
    }

    private void RuntimeControllerStateChanged(object? sender, EventArgs args)
    {
        var dispatcher = System.Windows.Application.Current?.Dispatcher;
        if (dispatcher is not null && !dispatcher.CheckAccess())
        {
            dispatcher.BeginInvoke(new Action(RefreshStatus));
            return;
        }
        RefreshStatus();
    }

    private void RefreshAllProperties()
    {
        foreach (var propertyName in new[]
        {
            nameof(UseDiscordWebhook),
            nameof(UseDiscordBot),
            nameof(UseMatrixOnly),
            nameof(UseHostedService),
            nameof(UsesLocalRuntime),
            nameof(CanToggleLocalMatrix),
            nameof(DiscordWebhookUrl),
            nameof(DiscordBotToken),
            nameof(DiscordApplicationId),
            nameof(DiscordChannelId),
            nameof(UseLocalMatrix),
            nameof(MatrixHomeserverUrl),
            nameof(MatrixUsername),
            nameof(MatrixPassword),
            nameof(MatrixRoomId),
            nameof(HostedServiceUrl),
            nameof(HostedCredentialReference),
            nameof(HostedCredentialStatus),
            nameof(LocalRuntimePath),
            nameof(UseDirectSource),
            nameof(UseAdvancedApiSource),
            nameof(AdvancedApiBaseUrl),
            nameof(PollingIntervalSeconds),
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
        startRuntimeCommand.RaiseCanExecuteChanged();
        stopRuntimeCommand.RaiseCanExecuteChanged();
    }

    private void RaisePropertyChanged([CallerMemberName] string? propertyName = null) =>
        PropertyChanged?.Invoke(this, new PropertyChangedEventArgs(propertyName));
}
