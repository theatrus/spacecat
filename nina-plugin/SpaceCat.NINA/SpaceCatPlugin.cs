using System.ComponentModel.Composition;
using NINA.Plugin;
using NINA.Plugin.Interfaces;

namespace SpaceCat.NINA;

/// <summary>
/// N.I.N.A. lifecycle entry point for SpaceCat.
///
/// Direct event subscriptions and sidecar supervision will be added behind
/// this manifest in follow-up changes. Keeping the manifest in the main
/// SpaceCat repository allows the native plugin and Rust protocol to be
/// released and tested together.
/// </summary>
[Export(typeof(IPluginManifest))]
public sealed class SpaceCatPlugin : PluginBase
{
    public override Task Initialize()
    {
        return base.Initialize();
    }

    public override Task Teardown()
    {
        return base.Teardown();
    }
}
