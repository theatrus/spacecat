using System.Text.Json;
using System.Text.Json.Serialization;

namespace Chatstronomy.NINA.Protocol;

internal static class DirectProtocol
{
    internal const ushort CurrentVersion = 1;
    private const string PipePrefix = "chatstronomy-agent-v1";
    internal const string WebSocketPath = "/v1/direct";

    internal static JsonSerializerOptions JsonOptions { get; } = new()
    {
        DefaultIgnoreCondition = JsonIgnoreCondition.WhenWritingNull,
        NumberHandling = JsonNumberHandling.AllowNamedFloatingPointLiterals,
    };

    static DirectProtocol()
    {
        JsonOptions.Converters.Add(new JsonStringEnumConverter());
    }

    internal static string LocalPipeName(Guid nodeId) => $"{PipePrefix}-{nodeId:N}";

    internal static DirectQuery ParseQuery(string json)
    {
        using var document = JsonDocument.Parse(json);
        var root = document.RootElement;
        if (RequiredString(root, "type") != "query")
        {
            throw new DirectProtocolException("Expected a Direct query message.");
        }

        var payload = RequiredObject(root, "payload");
        var id = RequiredGuid(payload, "id");
        var wireKind = RequiredString(payload, "kind");
        return wireKind switch
        {
            "event_history" => new DirectQuery(id, DirectQueryKind.EventHistory),
            "image_history" => new DirectQuery(id, DirectQueryKind.ImageHistory),
            "sequence" => new DirectQuery(id, DirectQueryKind.Sequence),
            "thumbnail" => new DirectQuery(
                id,
                DirectQueryKind.Thumbnail,
                Index: RequiredUInt32(payload, "index")),
            "last_autofocus" => new DirectQuery(id, DirectQueryKind.LastAutofocus),
            "mount_info" => new DirectQuery(id, DirectQueryKind.MountInfo),
            "filterwheel_info" => new DirectQuery(id, DirectQueryKind.FilterwheelInfo),
            "guider_info" => new DirectQuery(id, DirectQueryKind.GuiderInfo),
            "guider_graph" => new DirectQuery(id, DirectQueryKind.GuiderGraph),
            "rotator_info" => new DirectQuery(id, DirectQueryKind.RotatorInfo),
            "focuser_info" => new DirectQuery(id, DirectQueryKind.FocuserInfo),
            "command" => new DirectQuery(
                id,
                DirectQueryKind.Command,
                Command: ParseCommand(RequiredObject(payload, "command"))),
            _ => throw new DirectProtocolException($"Unsupported Direct query kind '{wireKind}'."),
        };
    }

    internal static string SerializeSuccess(Guid id, object? payload) =>
        JsonSerializer.Serialize(
            new DirectWireMessage<QueryResultPayload>(
                "query_result",
                new QueryResultPayload(
                    id,
                    Ok: true,
                    JsonSerializer.SerializeToElement(payload, JsonOptions),
                    Error: null)),
            JsonOptions);

    internal static string SerializeFailure(Guid id, string error) =>
        JsonSerializer.Serialize(
            new DirectWireMessage<QueryResultPayload>(
                "query_result",
                new QueryResultPayload(
                    id,
                    Ok: false,
                    JsonSerializer.SerializeToElement<object?>(null, JsonOptions),
                    string.IsNullOrWhiteSpace(error) ? "Direct query failed." : error)),
            JsonOptions);

    private static DirectRigCommand ParseCommand(JsonElement command)
    {
        var wireKind = RequiredString(command, "kind");
        return wireKind switch
        {
            "unpark_mount" => new DirectRigCommand(DirectRigCommandKind.UnparkMount),
            "home_mount" => new DirectRigCommand(DirectRigCommandKind.HomeMount),
            "change_filter" => new DirectRigCommand(
                DirectRigCommandKind.ChangeFilter,
                FilterId: RequiredInt32(command, "filter_id")),
            "start_guiding" => new DirectRigCommand(
                DirectRigCommandKind.StartGuiding,
                Calibrate: RequiredBoolean(command, "calibrate")),
            "stop_guiding" => new DirectRigCommand(DirectRigCommandKind.StopGuiding),
            "cool_camera" => new DirectRigCommand(
                DirectRigCommandKind.CoolCamera,
                Temperature: RequiredDouble(command, "temperature"),
                Minutes: RequiredDouble(command, "minutes")),
            "warm_camera" => new DirectRigCommand(
                DirectRigCommandKind.WarmCamera,
                Minutes: RequiredDouble(command, "minutes")),
            "start_autofocus" => new DirectRigCommand(DirectRigCommandKind.StartAutofocus),
            "cancel_autofocus" => new DirectRigCommand(DirectRigCommandKind.CancelAutofocus),
            "park_mount" => new DirectRigCommand(DirectRigCommandKind.ParkMount),
            "abort_exposure" => new DirectRigCommand(DirectRigCommandKind.AbortExposure),
            "stop_sequence" => new DirectRigCommand(DirectRigCommandKind.StopSequence),
            "start_sequence" => new DirectRigCommand(
                DirectRigCommandKind.StartSequence,
                SkipValidation: RequiredBoolean(command, "skip_validation")),
            _ => throw new DirectProtocolException(
                $"Unsupported Direct command kind '{wireKind}'."),
        };
    }

    private static JsonElement RequiredObject(JsonElement parent, string name)
    {
        if (!parent.TryGetProperty(name, out var value)
            || value.ValueKind != JsonValueKind.Object)
        {
            throw new DirectProtocolException($"Direct message field '{name}' must be an object.");
        }
        return value;
    }

    private static string RequiredString(JsonElement parent, string name)
    {
        if (!parent.TryGetProperty(name, out var value)
            || value.ValueKind != JsonValueKind.String
            || string.IsNullOrWhiteSpace(value.GetString()))
        {
            throw new DirectProtocolException($"Direct message field '{name}' must be a string.");
        }
        return value.GetString()!;
    }

    private static Guid RequiredGuid(JsonElement parent, string name)
    {
        var value = RequiredString(parent, name);
        if (!Guid.TryParse(value, out var result) || result == Guid.Empty)
        {
            throw new DirectProtocolException($"Direct message field '{name}' must be a non-empty UUID.");
        }
        return result;
    }

    private static uint RequiredUInt32(JsonElement parent, string name)
    {
        if (!parent.TryGetProperty(name, out var value) || !value.TryGetUInt32(out var result))
        {
            throw new DirectProtocolException($"Direct message field '{name}' must be an unsigned integer.");
        }
        return result;
    }

    private static int RequiredInt32(JsonElement parent, string name)
    {
        if (!parent.TryGetProperty(name, out var value) || !value.TryGetInt32(out var result))
        {
            throw new DirectProtocolException($"Direct message field '{name}' must be an integer.");
        }
        return result;
    }

    private static bool RequiredBoolean(JsonElement parent, string name)
    {
        if (!parent.TryGetProperty(name, out var value)
            || value.ValueKind is not (JsonValueKind.True or JsonValueKind.False))
        {
            throw new DirectProtocolException($"Direct message field '{name}' must be a boolean.");
        }
        return value.GetBoolean();
    }

    private static double RequiredDouble(JsonElement parent, string name)
    {
        if (!parent.TryGetProperty(name, out var value)
            || !value.TryGetDouble(out var result)
            || !double.IsFinite(result))
        {
            throw new DirectProtocolException($"Direct message field '{name}' must be a finite number.");
        }
        return result;
    }
}

internal enum DirectQueryKind
{
    EventHistory,
    ImageHistory,
    Sequence,
    Thumbnail,
    LastAutofocus,
    MountInfo,
    FilterwheelInfo,
    GuiderInfo,
    GuiderGraph,
    RotatorInfo,
    FocuserInfo,
    Command,
}

internal sealed record DirectQuery(
    Guid Id,
    DirectQueryKind Kind,
    uint? Index = null,
    DirectRigCommand? Command = null);

internal enum DirectRigCommandKind
{
    UnparkMount,
    HomeMount,
    ChangeFilter,
    StartGuiding,
    StopGuiding,
    CoolCamera,
    WarmCamera,
    StartAutofocus,
    CancelAutofocus,
    ParkMount,
    AbortExposure,
    StopSequence,
    StartSequence,
}

internal sealed record DirectRigCommand(
    DirectRigCommandKind Kind,
    int? FilterId = null,
    bool? Calibrate = null,
    double? Temperature = null,
    double? Minutes = null,
    bool? SkipValidation = null);

internal sealed class DirectProtocolException(string message) : Exception(message);

internal sealed record DirectWireMessage<T>(
    [property: JsonPropertyName("type")] string Type,
    [property: JsonPropertyName("payload")] T Payload);

internal sealed record QueryResultPayload(
    [property: JsonPropertyName("id")] Guid Id,
    [property: JsonPropertyName("ok")] bool Ok,
    [property: JsonPropertyName("payload")] JsonElement Payload,
    [property: JsonPropertyName("error")] string? Error);

internal sealed record DirectApiEnvelope<T>(
    [property: JsonPropertyName("Response")] T Response,
    [property: JsonPropertyName("Error")] string Error,
    [property: JsonPropertyName("StatusCode")] int StatusCode,
    [property: JsonPropertyName("Success")] bool Success,
    [property: JsonPropertyName("Type")] string Type)
{
    internal static DirectApiEnvelope<T> Ok(T response) =>
        new(response, string.Empty, 200, true, "API");
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
