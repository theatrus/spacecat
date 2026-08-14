using System.Text.Json.Serialization;

namespace SpaceCat.NINA.Protocol;

internal static class DirectProtocol
{
    internal const ushort CurrentVersion = 1;
    private const string PipePrefix = "spacecat-agent-v1";
    internal const string WebSocketPath = "/v1/direct";

    internal static string LocalPipeName(Guid nodeId) => $"{PipePrefix}-{nodeId:N}";
}

internal sealed record DirectCapabilities(
    [property: JsonPropertyName("event_history")] bool EventHistory,
    [property: JsonPropertyName("image_history")] bool ImageHistory,
    [property: JsonPropertyName("thumbnails")] bool Thumbnails,
    [property: JsonPropertyName("sequence")] bool Sequence,
    [property: JsonPropertyName("equipment_snapshots")] bool EquipmentSnapshots,
    [property: JsonPropertyName("autofocus_details")] bool AutofocusDetails,
    [property: JsonPropertyName("guider_graph")] bool GuiderGraph,
    [property: JsonPropertyName("commands")] bool Commands)
{
    internal static DirectCapabilities None { get; } = new(
        EventHistory: false,
        ImageHistory: false,
        Thumbnails: false,
        Sequence: false,
        EquipmentSnapshots: false,
        AutofocusDetails: false,
        GuiderGraph: false,
        Commands: false);
}

internal sealed record ClientHello(
    [property: JsonPropertyName("protocol_version")] ushort ProtocolVersion,
    [property: JsonPropertyName("node_id")] Guid NodeId,
    [property: JsonPropertyName("session_id")] Guid SessionId,
    [property: JsonPropertyName("process_id")] int ProcessId,
    [property: JsonPropertyName("profile_id")] Guid ProfileId,
    [property: JsonPropertyName("profile_name")] string ProfileName,
    [property: JsonPropertyName("plugin_version")] string PluginVersion,
    [property: JsonPropertyName("nina_version")] string NinaVersion,
    [property: JsonPropertyName("capabilities")] DirectCapabilities Capabilities);
