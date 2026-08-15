using Chatstronomy.NINA.Configuration;

namespace Chatstronomy.NINA.Runtime;

/// <summary>
/// Boundary for the bundled/local runtime implementation. Configuration can
/// be completed now without coupling the options UI to process management or
/// placing delivery secrets on a command line or in a temporary JSON file.
/// </summary>
internal interface IChatstronomyRuntimeController
{
    bool IsRunning { get; }

    Task StartAsync(ChatstronomyConfiguration configuration, CancellationToken cancellationToken);

    Task StopAsync(CancellationToken cancellationToken);
}
