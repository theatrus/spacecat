using System.ComponentModel.Composition;
using NINA.Plugin;
using NINA.Plugin.Interfaces;
using NINA.Profile.Interfaces;
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
public sealed class ChatstronomyPlugin : PluginBase
{
    private readonly IProfileService profileService;
    private readonly Guid nodeId = NodeIdentityStore.LoadOrCreate();
    private readonly Guid sessionId = Guid.NewGuid();
    private DirectConnectionSettings connectionSettings = DirectConnectionSettings.Local;

    [ImportingConstructor]
    public ChatstronomyPlugin(IProfileService profileService)
    {
        this.profileService = profileService;
    }

    public override Task Initialize()
    {
        return base.Initialize();
    }

    public override Task Teardown()
    {
        return base.Teardown();
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
}
