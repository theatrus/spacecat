using Chatstronomy.NINA.Configuration;
using Chatstronomy.NINA.Direct;
using Chatstronomy.NINA.Protocol;
using Chatstronomy.NINA.Runtime;
using Chatstronomy.NINA.Settings;
using System.Text.Json;

namespace Chatstronomy.NINA.Tests;

internal static class Program
{
    private static int failures;

    public static async Task<int> Main()
    {
        Run("Matrix accepts HTTPS homeservers", MatrixAcceptsHttpsHomeserver);
        Run("Matrix rejects HTTP homeservers", MatrixRejectsHttpHomeserver);
        Run("Discord accepts complete webhook URLs", DiscordAcceptsCompleteWebhookUrls);
        Run("Discord rejects incomplete webhook URLs", DiscordRejectsIncompleteWebhookUrls);
        Run("Discord application ID is optional", DiscordApplicationIdIsOptional);
        Run("Advanced API polling settings are validated", AdvancedApiPollingIsValidated);
        Run("Direct source does not require Advanced API settings", DirectSourceNeedsNoAdvancedApi);
        Run("Plugin runtime bootstrap is source-explicit", PluginRuntimeBootstrapIsSourceExplicit);
        Run("Direct runtime bootstrap carries only its pipe", DirectRuntimeBootstrapCarriesOnlyPipe);
        Run("Direct commands use semantic wire names", DirectCommandsUseSemanticWireNames);
        Run("Direct query results match Rust envelope", DirectQueryResultsMatchRustEnvelope);
        Run("Direct histories stay insertion ordered and bounded", DirectHistoriesAreBounded);

        var runtimePath = Environment.GetEnvironmentVariable("CHATSTRONOMY_RUNTIME_EXE");
        if (!string.IsNullOrWhiteSpace(runtimePath) && File.Exists(runtimePath))
        {
            await RunAsync(
                "Plugin runtime starts and stops over its control pipe",
                () => PluginRuntimeStartsAndStops(runtimePath));
            await RunAsync(
                "Plugin runtime can detach when configured to outlive N.I.N.A.",
                () => PluginRuntimeDetaches(runtimePath));
            await RunAsync(
                "Plugin runtime queries the native Direct data pipe",
                () => PluginRuntimeUsesDirectPipe(runtimePath));
        }
        else
        {
            Console.WriteLine(
                "SKIP: Plugin runtime process integration (CHATSTRONOMY_RUNTIME_EXE is not set).");
        }

        if (failures == 0)
        {
            Console.WriteLine("All Chatstronomy N.I.N.A. configuration tests passed.");
            return 0;
        }

        Console.Error.WriteLine($"{failures} Chatstronomy N.I.N.A. configuration test(s) failed.");
        return 1;
    }

    private static void MatrixAcceptsHttpsHomeserver()
    {
        var homeserver = ChatstronomyConfigurationValidator.RequireMatrixHomeserver(
            "https://matrix.example.test:8448/");

        AssertEqual(Uri.UriSchemeHttps, homeserver.Scheme);
        AssertEqual("matrix.example.test", homeserver.Host);
    }

    private static void MatrixRejectsHttpHomeserver() =>
        AssertThrows<InvalidOperationException>(() =>
            ChatstronomyConfigurationValidator.RequireMatrixHomeserver(
                "http://matrix.example.test/"));

    private static void DiscordAcceptsCompleteWebhookUrls()
    {
        ChatstronomyConfigurationValidator.RequireDiscordWebhook(
            "https://discord.com/api/webhooks/123456789012345678/token_value");
        ChatstronomyConfigurationValidator.RequireDiscordWebhook(
            "https://discord.com/api/v10/webhooks/123456789012345678/token_value");
    }

    private static void DiscordRejectsIncompleteWebhookUrls()
    {
        foreach (var value in new[]
        {
            "https://discord.com/api/webhooks/",
            "https://discord.com/api/webhooks/123456789012345678",
            "https://discord.com/api/webhooks/not-a-number/token_value",
            "https://discord.com:8443/api/webhooks/123456789012345678/token_value",
        })
        {
            AssertThrows<InvalidOperationException>(() =>
                ChatstronomyConfigurationValidator.RequireDiscordWebhook(value));
        }
    }

    private static void DiscordApplicationIdIsOptional()
    {
        AssertEqual<ulong?>(null,
            ChatstronomyConfigurationValidator.OptionalDiscordSnowflake(
                string.Empty,
                "Discord application ID"));
        AssertEqual<ulong?>(123456789012345678,
            ChatstronomyConfigurationValidator.OptionalDiscordSnowflake(
                "123456789012345678",
                "Discord application ID"));
        AssertThrows<InvalidOperationException>(() =>
            ChatstronomyConfigurationValidator.OptionalDiscordSnowflake(
                "not-a-number",
                "Discord application ID"));
    }

    private static void AdvancedApiPollingIsValidated()
    {
        var configuration = ChatstronomyConfigurationValidator.BuildLocalRuntime(
            Environment.ProcessPath ?? "test-runtime.exe",
            RuntimeSourceMode.AdvancedApi,
            "http://127.0.0.1:1888/",
            "7",
            startWithNina: false,
            stopWithNina: true);
        var source = AssertType<AdvancedApiPollingSourceConfiguration>(configuration.Source);
        AssertEqual("http://127.0.0.1:1888/", source.BaseUrl.AbsoluteUri);
        AssertEqual<uint>(7, source.PollIntervalSeconds);

        AssertThrows<InvalidOperationException>(() =>
            ChatstronomyConfigurationValidator.BuildLocalRuntime(
                Environment.ProcessPath ?? "test-runtime.exe",
                RuntimeSourceMode.AdvancedApi,
                "http://127.0.0.1:1888/",
                "0",
                startWithNina: false,
                stopWithNina: true));
    }

    private static void DirectSourceNeedsNoAdvancedApi()
    {
        var configuration = ChatstronomyConfigurationValidator.BuildLocalRuntime(
            Environment.ProcessPath ?? "test-runtime.exe",
            RuntimeSourceMode.Direct,
            advancedApiBaseUrl: string.Empty,
            pollingIntervalSeconds: string.Empty,
            startWithNina: true,
            stopWithNina: false);

        AssertType<NinaDirectSourceConfiguration>(configuration.Source);
        AssertTrue(configuration.StopWithNina);
        AssertThrows<InvalidOperationException>(() =>
            ChatstronomyConfigurationValidator.BuildLocalRuntime(
                Environment.ProcessPath ?? "test-runtime.exe",
                RuntimeSourceMode.Direct,
                advancedApiBaseUrl: string.Empty,
                pollingIntervalSeconds: string.Empty,
                startWithNina: false,
                stopWithNina: false));
    }

    private static void PluginRuntimeBootstrapIsSourceExplicit()
    {
        var configuration = BuildRuntimeConfiguration(
            Environment.ProcessPath ?? "test-runtime.exe");
        var json = PluginRuntimeBootstrap.Serialize(
            configuration,
            new LocalRuntimeIdentity(
                Guid.Parse("363db028-9d79-4fdc-8940-1b1ff52b9e8d"),
                Guid.Parse("460a8c62-28ce-4781-92e5-ab2440982175"),
                "North Rig"));

        using var document = JsonDocument.Parse(json);
        var root = document.RootElement;
        AssertEqual(PluginRuntimeBootstrap.ProtocolVersion,
            root.GetProperty("protocol_version").GetUInt16());
        AssertEqual("advanced_api_polling",
            root.GetProperty("source").GetProperty("kind").GetString());
        AssertEqual<uint>(5,
            root.GetProperty("source").GetProperty("poll_interval_seconds").GetUInt32());
        AssertEqual("discord_webhook",
            root.GetProperty("delivery").GetProperty("kind").GetString());
        AssertFalse(json.Contains("test-runtime.exe", StringComparison.OrdinalIgnoreCase));
    }

    private static void DirectCommandsUseSemanticWireNames()
    {
        const string json = """
            {
              "type": "query",
              "payload": {
                "id": "7afcde18-b5a8-46fd-ad1f-ed54cf3bbc4e",
                "kind": "command",
                "command": {
                  "kind": "cool_camera",
                  "temperature": -10.0,
                  "minutes": 15.0
                }
              }
            }
            """;

        var query = DirectProtocol.ParseQuery(json);
        AssertEqual(DirectQueryKind.Command, query.Kind);
        AssertEqual(DirectRigCommandKind.CoolCamera, query.Command!.Kind);
        AssertEqual(-10.0, query.Command.Temperature);
        AssertEqual(15.0, query.Command.Minutes);
        AssertFalse(json.Contains("/equipment/", StringComparison.Ordinal));

        AssertThrows<DirectProtocolException>(() => DirectProtocol.ParseQuery(
            json.Replace("cool_camera", "/equipment/camera/cool")));
    }

    private static void DirectRuntimeBootstrapCarriesOnlyPipe()
    {
        var runtimePath = Environment.ProcessPath ?? "test-runtime.exe";
        var configuration = new ChatstronomyConfiguration(
            new DiscordWebhookDeliveryConfiguration(
                new Uri("https://discord.com/api/webhooks/123/token")),
            Matrix: null,
            new LocalRuntimeConfiguration(
                runtimePath,
                new NinaDirectSourceConfiguration(),
                StartWithNina: false,
                StopWithNina: false));
        var json = PluginRuntimeBootstrap.Serialize(
            configuration,
            new LocalRuntimeIdentity(Guid.NewGuid(), Guid.NewGuid(), "Direct Rig"),
            directPipeName: "chatstronomy-direct-test",
            directCapabilities: new DirectCapabilities(
                EventHistory: true,
                ImageHistory: true,
                Thumbnails: false,
                Sequence: false,
                EquipmentSnapshots: true,
                AutofocusDetails: false,
                GuiderGraph: false,
                Commands: false));

        using var document = JsonDocument.Parse(json);
        var source = document.RootElement.GetProperty("source");
        AssertEqual("nina_direct", source.GetProperty("kind").GetString());
        AssertEqual("chatstronomy-direct-test", source.GetProperty("pipe_name").GetString());
        AssertTrue(source.GetProperty("capabilities").GetProperty("event_history").GetBoolean());
        AssertFalse(source.TryGetProperty("base_url", out _));
        AssertFalse(json.Contains("127.0.0.1:1888", StringComparison.Ordinal));
    }

    private static void DirectQueryResultsMatchRustEnvelope()
    {
        var id = Guid.Parse("7afcde18-b5a8-46fd-ad1f-ed54cf3bbc4e");
        var json = DirectProtocol.SerializeSuccess(
            id,
            DirectApiEnvelope<IReadOnlyList<object>>.Ok(Array.Empty<object>()));

        using var document = JsonDocument.Parse(json);
        var root = document.RootElement;
        AssertEqual("query_result", root.GetProperty("type").GetString());
        var payload = root.GetProperty("payload");
        AssertEqual(id, payload.GetProperty("id").GetGuid());
        AssertTrue(payload.GetProperty("ok").GetBoolean());
        var envelope = payload.GetProperty("payload");
        AssertTrue(envelope.GetProperty("Success").GetBoolean());
        AssertEqual("API", envelope.GetProperty("Type").GetString());
        AssertEqual(0, envelope.GetProperty("Response").GetArrayLength());
    }

    private static void DirectHistoriesAreBounded()
    {
        var history = new BoundedHistory<int>(capacity: 2);
        history.Add(1);
        history.Add(2);
        history.Add(3);

        AssertEqual(2, history.Count);
        AssertTrue(history.Snapshot().SequenceEqual(new[] { 2, 3 }));
        AssertTrue(history.TryGetAt(1, out var item));
        AssertEqual(3, item);
        AssertFalse(history.TryGetAt(2, out _));
        history.Clear();
        AssertEqual(0, history.Count);
    }

    private static async Task PluginRuntimeStartsAndStops(string runtimePath)
    {
        var controller = new ChatstronomyRuntimeController();
        var profileId = Guid.NewGuid();
        try
        {
            await controller.StartAsync(
                BuildRuntimeConfiguration(runtimePath),
                new LocalRuntimeIdentity(
                    Guid.NewGuid(),
                    profileId,
                    "Controller Integration Test"),
                CancellationToken.None);
            AssertTrue(controller.IsRunning);
            AssertTrue(controller.ProcessId.HasValue);

            await controller.StopAsync(CancellationToken.None);
            AssertFalse(controller.IsRunning);

            var logPath = Path.Combine(
                Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
                "Chatstronomy",
                "logs",
                $"nina-runtime-{profileId:N}.log");
            var log = File.ReadAllText(logPath);
            AssertFalse(log.Contains("api/webhooks", StringComparison.OrdinalIgnoreCase));
            AssertFalse(log.Contains("token", StringComparison.OrdinalIgnoreCase));
        }
        finally
        {
            if (controller.IsRunning)
            {
                await controller.StopAsync(CancellationToken.None);
            }
        }
    }

    private static async Task PluginRuntimeDetaches(string runtimePath)
    {
        var controller = new ChatstronomyRuntimeController();
        int? processId = null;
        try
        {
            await controller.StartAsync(
                BuildRuntimeConfiguration(runtimePath, stopWithNina: false),
                new LocalRuntimeIdentity(Guid.NewGuid(), Guid.NewGuid(), "Detach Test"),
                CancellationToken.None);
            processId = controller.ProcessId;
            AssertTrue(processId.HasValue);

            await controller.DetachAsync(CancellationToken.None);
            AssertFalse(controller.IsRunning);

            await Task.Delay(250);
            using var detached = System.Diagnostics.Process.GetProcessById(processId!.Value);
            AssertFalse(detached.HasExited);
            detached.Kill(entireProcessTree: true);
            await detached.WaitForExitAsync();
        }
        finally
        {
            if (controller.IsRunning)
            {
                await controller.StopAsync(CancellationToken.None);
            }
            if (processId.HasValue)
            {
                try
                {
                    using var detached = System.Diagnostics.Process.GetProcessById(processId.Value);
                    if (!detached.HasExited)
                    {
                        detached.Kill(entireProcessTree: true);
                        await detached.WaitForExitAsync();
                    }
                }
                catch (ArgumentException)
                {
                    // Already exited and removed from the process table.
                }
            }
        }
    }

    private static async Task PluginRuntimeUsesDirectPipe(string runtimePath)
    {
        var provider = new FakeDirectDataProvider();
        var controller = new ChatstronomyRuntimeController(provider);
        try
        {
            provider.Start();
            await controller.StartAsync(
                BuildDirectRuntimeConfiguration(runtimePath),
                new LocalRuntimeIdentity(Guid.NewGuid(), Guid.NewGuid(), "Direct Pipe Test"),
                CancellationToken.None);
            AssertTrue(controller.IsRunning);

            using var timeout = new CancellationTokenSource(TimeSpan.FromSeconds(10));
            while (provider.QueryCount < 2)
            {
                await Task.Delay(50, timeout.Token);
            }

            await controller.StopAsync(CancellationToken.None);
            AssertFalse(controller.IsRunning);
        }
        finally
        {
            if (controller.IsRunning)
            {
                await controller.StopAsync(CancellationToken.None);
            }
            provider.Dispose();
        }
    }

    private static ChatstronomyConfiguration BuildRuntimeConfiguration(
        string runtimePath,
        bool stopWithNina = true) =>
        new(
            new DiscordWebhookDeliveryConfiguration(
                new Uri("https://discord.com/api/webhooks/123/token")),
            Matrix: null,
            new LocalRuntimeConfiguration(
                runtimePath,
                new AdvancedApiPollingSourceConfiguration(
                    new Uri("http://127.0.0.1:1888/"),
                    PollIntervalSeconds: 5),
                StartWithNina: true,
                StopWithNina: stopWithNina));

    private static ChatstronomyConfiguration BuildDirectRuntimeConfiguration(string runtimePath) =>
        new(
            new DiscordWebhookDeliveryConfiguration(
                new Uri("https://discord.com/api/webhooks/123/token")),
            Matrix: null,
            new LocalRuntimeConfiguration(
                runtimePath,
                new NinaDirectSourceConfiguration(),
                StartWithNina: true,
                StopWithNina: true));

    private static void Run(string name, Action test)
    {
        try
        {
            test();
            Console.WriteLine($"PASS: {name}");
        }
        catch (Exception exception)
        {
            failures++;
            Console.Error.WriteLine($"FAIL: {name}: {exception.Message}");
        }
    }

    private static async Task RunAsync(string name, Func<Task> test)
    {
        try
        {
            await test();
            Console.WriteLine($"PASS: {name}");
        }
        catch (Exception exception)
        {
            failures++;
            Console.Error.WriteLine($"FAIL: {name}: {exception.Message}");
        }
    }

    private static T AssertType<T>(object value)
    {
        if (value is T typed)
        {
            return typed;
        }

        throw new InvalidOperationException(
            $"Expected {typeof(T).Name}, but received {value.GetType().Name}.");
    }

    private static void AssertTrue(bool value)
    {
        if (!value)
        {
            throw new InvalidOperationException("Expected condition to be true.");
        }
    }

    private static void AssertFalse(bool value) => AssertTrue(!value);

    private static void AssertEqual<T>(T expected, T actual)
    {
        if (!EqualityComparer<T>.Default.Equals(expected, actual))
        {
            throw new InvalidOperationException(
                $"Expected '{expected}', but received '{actual}'.");
        }
    }

    private static void AssertThrows<TException>(Action action)
        where TException : Exception
    {
        try
        {
            action();
        }
        catch (TException)
        {
            return;
        }

        throw new InvalidOperationException(
            $"Expected {typeof(TException).Name} to be thrown.");
    }

    private sealed class FakeDirectDataProvider : INinaDirectDataProvider
    {
        private int queryCount;

        public DirectCapabilities Capabilities { get; } = new(
            EventHistory: true,
            ImageHistory: true,
            Thumbnails: false,
            Sequence: false,
            EquipmentSnapshots: false,
            AutofocusDetails: false,
            GuiderGraph: false,
            Commands: false);

        public int QueryCount => Volatile.Read(ref queryCount);

        public void Start()
        {
        }

        public void Stop()
        {
        }

        public void Reset()
        {
        }

        public Task<object?> ExecuteAsync(
            DirectQuery query,
            CancellationToken cancellationToken)
        {
            cancellationToken.ThrowIfCancellationRequested();
            Interlocked.Increment(ref queryCount);
            object response = query.Kind switch
            {
                DirectQueryKind.EventHistory =>
                    DirectApiEnvelope<IReadOnlyList<object>>.Ok(Array.Empty<object>()),
                DirectQueryKind.ImageHistory =>
                    DirectApiEnvelope<IReadOnlyList<object>>.Ok(Array.Empty<object>()),
                _ => throw new NotSupportedException(),
            };
            return Task.FromResult<object?>(response);
        }

        public void Dispose() => Stop();
    }
}
