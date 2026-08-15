using Chatstronomy.NINA.Protocol;
using System.IO;
using System.Text.Json.Serialization;
using System.Windows.Media;
using System.Windows.Media.Imaging;
using NINA.Core.Interfaces;
using NINA.Equipment.Equipment.MyFocuser;
using NINA.Equipment.Interfaces;
using NINA.Equipment.Interfaces.Mediator;
using NINA.Profile.Interfaces;
using NINA.Sequencer.Interfaces.Mediator;
using NINA.WPF.Base.Interfaces.Mediator;
using OxyPlot;

namespace Chatstronomy.NINA.Direct;

/// <summary>
/// Native implementation of the Advanced API-compatible read surface used by
/// Chatstronomy. It reads live device state from N.I.N.A. mediators and keeps
/// only bounded callback history; it does not host an HTTP server.
/// </summary>
internal sealed class NinaDirectDataProvider : INinaDirectDataProvider, IFocuserConsumer
{
    private const int EventHistoryCapacity = 2_000;
    private const int ImageHistoryCapacity = 500;
    private const int GuideHistoryCapacity = 500;

    private readonly IProfileService profileService;
    private readonly ITelescopeMediator telescope;
    private readonly ICameraMediator camera;
    private readonly IFilterWheelMediator filterWheel;
    private readonly IGuiderMediator guider;
    private readonly IRotatorMediator rotator;
    private readonly IFocuserMediator focuser;
    private readonly ISequenceMediator sequence;
    private readonly IImageSaveMediator imageSave;
    private readonly BoundedHistory<Dictionary<string, object?>> events =
        new(EventHistoryCapacity);
    private readonly BoundedHistory<DirectSavedImage> images =
        new(ImageHistoryCapacity);
    private readonly BoundedHistory<DirectGuideStep> guideSteps =
        new(GuideHistoryCapacity);
    private readonly object sequenceGate = new();
    private CancellationTokenSource? sequenceSubscriptionStop;
    private bool sequenceSubscribed;
    private long guideStepId;
    private bool started;

    internal NinaDirectDataProvider(
        IProfileService profileService,
        ITelescopeMediator telescope,
        ICameraMediator camera,
        IFilterWheelMediator filterWheel,
        IGuiderMediator guider,
        IRotatorMediator rotator,
        IFocuserMediator focuser,
        ISequenceMediator sequence,
        IImageSaveMediator imageSave)
    {
        this.profileService = profileService;
        this.telescope = telescope;
        this.camera = camera;
        this.filterWheel = filterWheel;
        this.guider = guider;
        this.rotator = rotator;
        this.focuser = focuser;
        this.sequence = sequence;
        this.imageSave = imageSave;
    }

    public DirectCapabilities Capabilities { get; } = new(
        EventHistory: true,
        ImageHistory: true,
        Thumbnails: true,
        Sequence: false,
        EquipmentSnapshots: true,
        AutofocusDetails: false,
        GuiderGraph: true,
        Commands: false);

    public void Start()
    {
        if (started)
        {
            return;
        }

        telescope.Connected += TelescopeConnected;
        telescope.Disconnected += TelescopeDisconnected;
        telescope.BeforeMeridianFlip += TelescopeBeforeMeridianFlip;
        telescope.AfterMeridianFlip += TelescopeAfterMeridianFlip;
        telescope.Homed += TelescopeHomed;
        telescope.Parked += TelescopeParked;
        telescope.Unparked += TelescopeUnparked;

        camera.Connected += CameraConnected;
        camera.Disconnected += CameraDisconnected;
        camera.DownloadTimeout += CameraDownloadTimeout;

        filterWheel.Connected += FilterWheelConnected;
        filterWheel.Disconnected += FilterWheelDisconnected;
        filterWheel.FilterChanged += FilterWheelChanged;

        guider.Connected += GuiderConnected;
        guider.Disconnected += GuiderDisconnected;
        guider.AfterDither += GuiderDithered;
        guider.GuidingStarted += GuiderStarted;
        guider.GuidingStopped += GuiderStopped;
        guider.GuideEvent += GuiderGuideEvent;

        rotator.Connected += RotatorConnected;
        rotator.Disconnected += RotatorDisconnected;
        rotator.Moved += RotatorMoved;
        rotator.MovedMechanical += RotatorMovedMechanical;
        rotator.Synced += RotatorSynced;

        focuser.Connected += FocuserConnected;
        focuser.Disconnected += FocuserDisconnected;
        focuser.RegisterConsumer(this);

        imageSave.ImageSaved += ImageSaved;
        started = true;
        sequenceSubscriptionStop = new CancellationTokenSource();
        _ = SubscribeToSequenceWhenReadyAsync(sequenceSubscriptionStop.Token);
    }

    public void Stop()
    {
        if (!started)
        {
            return;
        }

        telescope.Connected -= TelescopeConnected;
        telescope.Disconnected -= TelescopeDisconnected;
        telescope.BeforeMeridianFlip -= TelescopeBeforeMeridianFlip;
        telescope.AfterMeridianFlip -= TelescopeAfterMeridianFlip;
        telescope.Homed -= TelescopeHomed;
        telescope.Parked -= TelescopeParked;
        telescope.Unparked -= TelescopeUnparked;

        camera.Connected -= CameraConnected;
        camera.Disconnected -= CameraDisconnected;
        camera.DownloadTimeout -= CameraDownloadTimeout;

        filterWheel.Connected -= FilterWheelConnected;
        filterWheel.Disconnected -= FilterWheelDisconnected;
        filterWheel.FilterChanged -= FilterWheelChanged;

        guider.Connected -= GuiderConnected;
        guider.Disconnected -= GuiderDisconnected;
        guider.AfterDither -= GuiderDithered;
        guider.GuidingStarted -= GuiderStarted;
        guider.GuidingStopped -= GuiderStopped;
        guider.GuideEvent -= GuiderGuideEvent;

        rotator.Connected -= RotatorConnected;
        rotator.Disconnected -= RotatorDisconnected;
        rotator.Moved -= RotatorMoved;
        rotator.MovedMechanical -= RotatorMovedMechanical;
        rotator.Synced -= RotatorSynced;

        focuser.Connected -= FocuserConnected;
        focuser.Disconnected -= FocuserDisconnected;
        focuser.RemoveConsumer(this);

        started = false;
        var subscriptionStop = sequenceSubscriptionStop;
        sequenceSubscriptionStop = null;
        subscriptionStop?.Cancel();
        lock (sequenceGate)
        {
            if (sequenceSubscribed)
            {
                sequence.SequenceStarting -= SequenceStarting;
                sequence.SequenceFinished -= SequenceFinished;
                sequenceSubscribed = false;
            }
        }
        subscriptionStop?.Dispose();
        imageSave.ImageSaved -= ImageSaved;
    }

    public void Reset()
    {
        events.Clear();
        images.Clear();
        guideSteps.Clear();
        Interlocked.Exchange(ref guideStepId, 0);
    }

    public Task<object?> ExecuteAsync(
        DirectQuery query,
        CancellationToken cancellationToken)
    {
        cancellationToken.ThrowIfCancellationRequested();
        object? result = query.Kind switch
        {
            DirectQueryKind.EventHistory =>
                DirectApiEnvelope<IReadOnlyList<Dictionary<string, object?>>>.Ok(events.Snapshot()),
            DirectQueryKind.ImageHistory =>
                DirectApiEnvelope<IReadOnlyList<DirectImageMetadata>>.Ok(
                    images.Snapshot().Select(image => image.Metadata).ToArray()),
            DirectQueryKind.Thumbnail => GetThumbnail(query.Index),
            DirectQueryKind.MountInfo =>
                DirectApiEnvelope<IReadOnlyDictionary<string, object?>>.Ok(GetMountInfo()),
            DirectQueryKind.FilterwheelInfo =>
                DirectApiEnvelope<DirectFilterWheelInfo>.Ok(GetFilterWheelInfo()),
            DirectQueryKind.GuiderInfo =>
                DirectApiEnvelope<DirectGuiderInfo>.Ok(GetGuiderInfo()),
            DirectQueryKind.GuiderGraph =>
                DirectApiEnvelope<DirectGuiderGraph>.Ok(GetGuiderGraph()),
            DirectQueryKind.RotatorInfo => DirectApiEnvelope<object>.Ok(rotator.GetInfo()),
            DirectQueryKind.FocuserInfo => DirectApiEnvelope<object>.Ok(focuser.GetInfo()),
            _ => throw new NotSupportedException(
                $"Direct query '{query.Kind}' is not implemented by this plugin version."),
        };
        return Task.FromResult<object?>(result);
    }

    public void Dispose() => Stop();

    public void UpdateDeviceInfo(FocuserInfo deviceInfo)
    {
    }

    public void UpdateEndAutoFocusRun(AutoFocusInfo info) => AddEvent("AUTOFOCUS-FINISHED");

    public void UpdateUserFocused(FocuserInfo info) => AddEvent("FOCUSER-USER-FOCUSED");

    public void AutoFocusRunStarting() => AddEvent("AUTOFOCUS-STARTING");

    public void NewAutoFocusPoint(DataPoint dataPoint) => AddEvent(
        "AUTOFOCUS-POINT-ADDED",
        ("Position", FiniteInt(dataPoint.X)),
        ("HFR", FiniteOrZero(dataPoint.Y)));

    private IReadOnlyDictionary<string, object?> GetMountInfo()
    {
        var info = telescope.GetInfo();
        var coordinates = info.Coordinates;
        var coordinateRa = FiniteOrZero(coordinates?.RA ?? info.RightAscension);
        var coordinateDec = FiniteOrZero(coordinates?.Dec ?? info.Declination);
        var coordinateRaDegrees = FiniteOrZero(coordinates?.RADegrees ?? coordinateRa * 15);
        var coordinateEpoch = coordinates?.Epoch.ToString() ?? info.EquatorialSystem.ToString();
        var now = coordinates?.DateTime.Now ?? DateTime.Now;
        var utcNow = coordinates?.DateTime.UtcNow ?? DateTime.UtcNow;
        var emptyObject = new Dictionary<string, object?>();

        return new Dictionary<string, object?>
        {
            ["SiderealTime"] = FiniteOrZero(info.SiderealTime),
            ["RightAscension"] = FiniteOrZero(info.RightAscension),
            ["Declination"] = FiniteOrZero(info.Declination),
            ["SiteLatitude"] = FiniteOrZero(info.SiteLatitude),
            ["SiteLongitude"] = FiniteOrZero(info.SiteLongitude),
            ["SiteElevation"] = FiniteInt(info.SiteElevation),
            ["RightAscensionString"] = info.RightAscensionString ?? string.Empty,
            ["DeclinationString"] = info.DeclinationString ?? string.Empty,
            ["Coordinates"] = new Dictionary<string, object?>
            {
                ["RA"] = coordinateRa,
                ["RAString"] = coordinates?.RAString ?? info.RightAscensionString ?? string.Empty,
                ["RADegrees"] = coordinateRaDegrees,
                ["Dec"] = coordinateDec,
                ["DecString"] = coordinates?.DecString ?? info.DeclinationString ?? string.Empty,
                ["Epoch"] = coordinateEpoch,
                ["DateTime"] = new Dictionary<string, object?>
                {
                    ["Now"] = now,
                    ["UtcNow"] = utcNow,
                },
            },
            ["TimeToMeridianFlip"] = FiniteOrZero(info.TimeToMeridianFlip),
            ["SideOfPier"] = info.SideOfPier.ToString(),
            ["Altitude"] = FiniteOrZero(info.Altitude),
            ["AltitudeString"] = info.AltitudeString ?? string.Empty,
            ["Azimuth"] = FiniteOrZero(info.Azimuth),
            ["AzimuthString"] = info.AzimuthString ?? string.Empty,
            ["SiderealTimeString"] = info.SiderealTimeString ?? string.Empty,
            ["HoursToMeridianString"] = info.HoursToMeridianString ?? string.Empty,
            ["AtPark"] = info.AtPark,
            ["TrackingRate"] = emptyObject,
            ["TrackingEnabled"] = info.TrackingEnabled,
            ["TrackingModes"] = info.TrackingModes?.Select(mode => mode.ToString()).ToArray()
                ?? Array.Empty<string>(),
            ["AtHome"] = info.AtHome,
            ["CanFindHome"] = info.CanFindHome,
            ["CanPark"] = info.CanPark,
            ["CanSetPark"] = info.CanSetPark,
            ["CanSetTrackingEnabled"] = info.CanSetTrackingEnabled,
            ["CanSetDeclinationRate"] = info.CanSetDeclinationRate,
            ["CanSetRightAscensionRate"] = info.CanSetRightAscensionRate,
            ["EquatorialSystem"] = info.EquatorialSystem.ToString(),
            ["HasUnknownEpoch"] = info.HasUnknownEpoch,
            ["TimeToMeridianFlipString"] = info.TimeToMeridianFlipString ?? string.Empty,
            ["Slewing"] = info.Slewing,
            ["GuideRateRightAscensionArcsecPerSec"] =
                FiniteOrZero(info.GuideRateRightAscensionArcsecPerSec),
            ["GuideRateDeclinationArcsecPerSec"] =
                FiniteOrZero(info.GuideRateDeclinationArcsecPerSec),
            ["CanMovePrimaryAxis"] = info.CanMovePrimaryAxis,
            ["CanMoveSecondaryAxis"] = info.CanMoveSecondaryAxis,
            ["PrimaryAxisRates"] = info.PrimaryAxisRates?.Select(_ => emptyObject).ToArray()
                ?? Array.Empty<Dictionary<string, object?>>(),
            ["SecondaryAxisRates"] = info.SecondaryAxisRates?.Select(_ => emptyObject).ToArray()
                ?? Array.Empty<Dictionary<string, object?>>(),
            ["SupportedActions"] = info.SupportedActions?.ToArray() ?? Array.Empty<string>(),
            ["AlignmentMode"] = info.AlignmentMode.ToString(),
            ["CanPulseGuide"] = info.CanPulseGuide,
            ["IsPulseGuiding"] = info.IsPulseGuiding,
            ["CanSetPierSide"] = info.CanSetPierSide,
            ["CanSlew"] = info.CanSlew,
            ["UTCDate"] = info.UTCDate,
            ["Connected"] = info.Connected,
            ["Name"] = info.Name ?? string.Empty,
            ["DisplayName"] = info.DisplayName ?? string.Empty,
            ["DeviceId"] = info.DeviceId ?? string.Empty,
        };
    }

    private DirectFilterWheelInfo GetFilterWheelInfo()
    {
        var info = filterWheel.GetInfo();
        var available = profileService.ActiveProfile.FilterWheelSettings.FilterWheelFilters
            .Select(filter => new DirectFilterInfo(filter.Name ?? string.Empty, filter.Position))
            .ToArray();
        var selected = info.Connected && info.SelectedFilter is { } filter
            ? new DirectFilterInfo(filter.Name ?? string.Empty, filter.Position)
            : null;
        return new DirectFilterWheelInfo(
            info.Connected,
            info.Name ?? string.Empty,
            info.DisplayName ?? string.Empty,
            info.IsMoving,
            selected,
            available);
    }

    private DirectGuiderInfo GetGuiderInfo()
    {
        var info = guider.GetInfo();
        var state = (guider.GetDevice() as IGuider)?.State ?? string.Empty;
        return new DirectGuiderInfo(
            info.Connected,
            info.Name ?? string.Empty,
            info.DisplayName ?? string.Empty,
            state,
            FiniteOrZero(info.PixelScale),
            info.RMSError);
    }

    private DirectGuiderGraph GetGuiderGraph()
    {
        var info = guider.GetInfo();
        var pixelScale = FiniteOrZero(info.PixelScale);
        var steps = guideSteps.Snapshot();
        var rms = steps.Count == 0 ? null : DirectGuideRms.FromSteps(steps, pixelScale);
        var maxDistance = steps.Count == 0
            ? 0
            : steps.Max(step => Math.Max(
                Math.Abs(step.RADistanceRawDisplay),
                Math.Abs(step.DECDistanceRawDisplay)));
        var maxDuration = steps.Count == 0
            ? 0
            : steps.Max(step => Math.Max(Math.Abs(step.RADuration), Math.Abs(step.DECDuration)));
        return new DirectGuiderGraph(
            rms,
            Interval: 1,
            MaxY: maxDistance,
            MinY: -maxDistance,
            MaxDurationY: maxDuration,
            MinDurationY: -maxDuration,
            GuideSteps: steps,
            HistorySize: GuideHistoryCapacity,
            PixelScale: pixelScale,
            Scale: 1);
    }

    private DirectThumbnail GetThumbnail(uint? index)
    {
        if (!index.HasValue || index.Value > int.MaxValue
            || !images.TryGetAt((int)index.Value, out var savedImage)
            || savedImage is null)
        {
            throw new InvalidOperationException("The requested image is no longer in history.");
        }

        var data = savedImage.ThumbnailData;
        if (data is null)
        {
            throw new InvalidOperationException("The image thumbnail is still being prepared.");
        }
        return new DirectThumbnail(data, "image/jpeg", 200);
    }

    private void ImageSaved(object? sender, ImageSavedEventArgs args)
    {
        var metadata = args.MetaData;
        var statistics = args.Statistics;
        var starAnalysis = args.StarDetectionAnalysis;
        var image = new DirectImageMetadata(
            ExposureTime: FiniteOrZero(args.Duration),
            ImageType: metadata?.Image?.ImageType ?? string.Empty,
            Filter: args.Filter ?? string.Empty,
            RmsText: metadata?.Image?.RecordedRMS?.TotalText ?? string.Empty,
            Temperature: FiniteOrZero(metadata?.Camera?.Temperature ?? 0),
            CameraName: metadata?.Camera?.Name ?? string.Empty,
            Gain: metadata?.Camera?.Gain ?? -1,
            Offset: metadata?.Camera?.Offset ?? -1,
            Date: DateTime.Now,
            TelescopeName: metadata?.Telescope?.Name ?? string.Empty,
            FocalLength: FiniteInt(metadata?.Telescope?.FocalLength ?? 0),
            StDev: FiniteOrZero(statistics?.StDev ?? 0),
            Mean: FiniteOrZero(statistics?.Mean ?? 0),
            Median: FiniteOrZero(statistics?.Median ?? 0),
            Stars: starAnalysis?.DetectedStars ?? 0,
            HFR: FiniteOrZero(starAnalysis?.HFR ?? 0),
            IsBayered: args.IsBayered);
        var savedImage = new DirectSavedImage(image);
        images.Add(savedImage);
        AddEvent("IMAGE-SAVE");
        QueueThumbnail(args.Image, savedImage);
    }

    private static void QueueThumbnail(BitmapSource? source, DirectSavedImage savedImage)
    {
        if (source is null)
        {
            return;
        }

        BitmapSource frozen = source;
        if (!source.IsFrozen)
        {
            frozen = source.Clone();
            frozen.Freeze();
        }

        _ = Task.Run(() =>
        {
            try
            {
                var scale = frozen.PixelWidth > 256 ? 256d / frozen.PixelWidth : 1d;
                BitmapSource thumbnail = frozen;
                if (scale < 1)
                {
                    var transformed = new TransformedBitmap(
                        frozen,
                        new ScaleTransform(scale, scale));
                    transformed.Freeze();
                    thumbnail = transformed;
                }

                var encoder = new JpegBitmapEncoder { QualityLevel = 85 };
                encoder.Frames.Add(BitmapFrame.Create(thumbnail));
                using var stream = new MemoryStream();
                encoder.Save(stream);
                savedImage.ThumbnailData = stream.ToArray();
            }
            catch
            {
                // The metadata remains useful and the runtime degrades to a
                // notification without an attachment.
            }
        });
    }

    private void GuiderGuideEvent(object? sender, IGuideStep step)
    {
        var pixelScale = FiniteOrZero(guider.GetInfo().PixelScale);
        var id = Interlocked.Increment(ref guideStepId);
        guideSteps.Add(new DirectGuideStep(
            Id: id,
            IdOffsetLeft: id - 0.4,
            IdOffsetRight: id + 0.4,
            RADistanceRaw: FiniteOrZero(step.RADistanceRaw),
            RADistanceRawDisplay: FiniteOrZero(step.RADistanceRaw * pixelScale),
            RADuration: FiniteOrZero(step.RADuration),
            DECDistanceRaw: FiniteOrZero(step.DECDistanceRaw),
            DECDistanceRawDisplay: FiniteOrZero(step.DECDistanceRaw * pixelScale),
            DECDuration: FiniteOrZero(step.DECDuration),
            Dither: "NO"));
    }

    private void AddEvent(string eventName, params (string Name, object? Value)[] details)
    {
        var item = new Dictionary<string, object?>
        {
            ["Time"] = DateTime.Now,
            ["Event"] = eventName,
        };
        foreach (var (name, value) in details)
        {
            item[name] = value;
        }
        events.Add(item);
    }

    private async Task SubscribeToSequenceWhenReadyAsync(CancellationToken cancellationToken)
    {
        while (!cancellationToken.IsCancellationRequested)
        {
            try
            {
                // N.I.N.A.'s SequenceMediator.Initialized getter itself cannot
                // be called before sequence navigation has registered.
                if (sequence.Initialized)
                {
                    lock (sequenceGate)
                    {
                        if (!started || cancellationToken.IsCancellationRequested)
                        {
                            return;
                        }
                        sequence.SequenceStarting += SequenceStarting;
                        try
                        {
                            sequence.SequenceFinished += SequenceFinished;
                        }
                        catch
                        {
                            sequence.SequenceStarting -= SequenceStarting;
                            throw;
                        }
                        sequenceSubscribed = true;
                        return;
                    }
                }
            }
            catch (NullReferenceException)
            {
                // Sequence navigation has not registered yet.
            }

            try
            {
                await Task.Delay(TimeSpan.FromMilliseconds(250), cancellationToken)
                    .ConfigureAwait(false);
            }
            catch (OperationCanceledException) when (cancellationToken.IsCancellationRequested)
            {
                return;
            }
        }
    }

    private Task TelescopeConnected(object sender, EventArgs args) => AddSimpleEvent("MOUNT-CONNECTED");
    private Task TelescopeDisconnected(object sender, EventArgs args) => AddSimpleEvent("MOUNT-DISCONNECTED");
    private Task TelescopeBeforeMeridianFlip(object sender, BeforeMeridianFlipEventArgs args) => AddSimpleEvent("MOUNT-BEFORE-FLIP");
    private Task TelescopeAfterMeridianFlip(object sender, AfterMeridianFlipEventArgs args) => AddSimpleEvent("MOUNT-AFTER-FLIP");
    private Task TelescopeHomed(object sender, EventArgs args) => AddSimpleEvent("MOUNT-HOMED");
    private Task TelescopeParked(object sender, EventArgs args) => AddSimpleEvent("MOUNT-PARKED");
    private Task TelescopeUnparked(object sender, EventArgs args) => AddSimpleEvent("MOUNT-UNPARKED");
    private Task CameraConnected(object sender, EventArgs args) => AddSimpleEvent("CAMERA-CONNECTED");
    private Task CameraDisconnected(object sender, EventArgs args) => AddSimpleEvent("CAMERA-DISCONNECTED");
    private Task CameraDownloadTimeout(object sender, EventArgs args) => AddSimpleEvent("CAMERA-DOWNLOAD-TIMEOUT");
    private Task FilterWheelConnected(object sender, EventArgs args) => AddSimpleEvent("FILTERWHEEL-CONNECTED");
    private Task FilterWheelDisconnected(object sender, EventArgs args) => AddSimpleEvent("FILTERWHEEL-DISCONNECTED");
    private Task GuiderConnected(object sender, EventArgs args) => AddSimpleEvent("GUIDER-CONNECTED");
    private Task GuiderDisconnected(object sender, EventArgs args) => AddSimpleEvent("GUIDER-DISCONNECTED");
    private Task GuiderDithered(object sender, EventArgs args) => AddSimpleEvent("GUIDER-DITHER");
    private Task GuiderStarted(object sender, EventArgs args) => AddSimpleEvent("GUIDER-START");
    private Task GuiderStopped(object sender, EventArgs args) => AddSimpleEvent("GUIDER-STOP");
    private Task RotatorConnected(object sender, EventArgs args) => AddSimpleEvent("ROTATOR-CONNECTED");
    private Task RotatorDisconnected(object sender, EventArgs args) => AddSimpleEvent("ROTATOR-DISCONNECTED");
    private Task FocuserConnected(object sender, EventArgs args) => AddSimpleEvent("FOCUSER-CONNECTED");
    private Task FocuserDisconnected(object sender, EventArgs args) => AddSimpleEvent("FOCUSER-DISCONNECTED");
    private Task SequenceStarting(object sender, EventArgs args) => AddSimpleEvent("SEQUENCE-STARTING");
    private Task SequenceFinished(object sender, EventArgs args) => AddSimpleEvent("SEQUENCE-FINISHED");

    private Task FilterWheelChanged(object sender, FilterChangedEventArgs args)
    {
        AddEvent(
            "FILTERWHEEL-CHANGED",
            ("Previous", new DirectFilterInfo(args.From?.Name ?? string.Empty, args.From?.Position ?? -1)),
            ("New", new DirectFilterInfo(args.To?.Name ?? string.Empty, args.To?.Position ?? -1)));
        return Task.CompletedTask;
    }

    private Task RotatorMoved(object sender, RotatorEventArgs args)
    {
        AddEvent("ROTATOR-MOVED", ("From", args.From), ("To", args.To));
        return Task.CompletedTask;
    }

    private Task RotatorMovedMechanical(object sender, RotatorEventArgs args)
    {
        AddEvent("ROTATOR-MOVED-MECHANICAL", ("From", args.From), ("To", args.To));
        return Task.CompletedTask;
    }

    private void RotatorSynced(object? sender, RotatorEventArgs args) => AddEvent("ROTATOR-SYNCED");

    private Task AddSimpleEvent(string eventName)
    {
        AddEvent(eventName);
        return Task.CompletedTask;
    }

    private static double FiniteOrZero(double value) => double.IsFinite(value) ? value : 0;

    private static int FiniteInt(double value) =>
        double.IsFinite(value)
            ? (int)Math.Clamp(Math.Round(value), int.MinValue, int.MaxValue)
            : 0;
}

internal sealed record DirectFilterInfo(string Name, int Id);

internal sealed record DirectFilterWheelInfo(
    bool Connected,
    string Name,
    string DisplayName,
    bool IsMoving,
    DirectFilterInfo? SelectedFilter,
    IReadOnlyList<DirectFilterInfo> AvailableFilters);

internal sealed record DirectGuiderInfo(
    bool Connected,
    string Name,
    string DisplayName,
    string State,
    double PixelScale,
    object? RMSError);

internal sealed record DirectImageMetadata(
    double ExposureTime,
    string ImageType,
    string Filter,
    string RmsText,
    double Temperature,
    string CameraName,
    int Gain,
    int Offset,
    DateTime Date,
    string TelescopeName,
    int FocalLength,
    double StDev,
    double Mean,
    double Median,
    int Stars,
    double HFR,
    bool IsBayered);

internal sealed class DirectSavedImage(DirectImageMetadata metadata)
{
    internal DirectImageMetadata Metadata { get; } = metadata;

    internal byte[]? ThumbnailData { get; set; }
}

internal sealed record DirectThumbnail(
    [property: JsonPropertyName("data")] byte[] Data,
    [property: JsonPropertyName("content_type")] string ContentType,
    [property: JsonPropertyName("status_code")] ushort StatusCode);

internal sealed record DirectGuiderGraph(
    [property: JsonPropertyName("RMS")] DirectGuideRms? RMS,
    int Interval,
    double MaxY,
    double MinY,
    double MaxDurationY,
    double MinDurationY,
    IReadOnlyList<DirectGuideStep> GuideSteps,
    int HistorySize,
    double PixelScale,
    int Scale);

internal sealed record DirectGuideRms(
    [property: JsonPropertyName("RA")] double RA,
    double Dec,
    double Total,
    [property: JsonPropertyName("RAText")] string RAText,
    string DecText,
    string TotalText,
    [property: JsonPropertyName("PeakRAText")] string PeakRAText,
    string PeakDecText,
    double Scale,
    [property: JsonPropertyName("PeakRA")] double PeakRA,
    double PeakDec,
    int DataPoints)
{
    internal static DirectGuideRms FromSteps(
        IReadOnlyList<DirectGuideStep> steps,
        double pixelScale)
    {
        var ra = Math.Sqrt(steps.Average(step => Math.Pow(step.RADistanceRawDisplay, 2)));
        var dec = Math.Sqrt(steps.Average(step => Math.Pow(step.DECDistanceRawDisplay, 2)));
        var total = Math.Sqrt(ra * ra + dec * dec);
        var peakRa = steps.Max(step => Math.Abs(step.RADistanceRawDisplay));
        var peakDec = steps.Max(step => Math.Abs(step.DECDistanceRawDisplay));
        return new DirectGuideRms(
            ra,
            dec,
            total,
            $"RA: {ra:0.00}",
            $"Dec: {dec:0.00}",
            $"Tot: {total:0.00}",
            $"Peak RA: {peakRa:0.00}",
            $"Peak Dec: {peakDec:0.00}",
            pixelScale,
            peakRa,
            peakDec,
            steps.Count);
    }
}

internal sealed record DirectGuideStep(
    long Id,
    double IdOffsetLeft,
    double IdOffsetRight,
    [property: JsonPropertyName("RADistanceRaw")] double RADistanceRaw,
    [property: JsonPropertyName("RADistanceRawDisplay")] double RADistanceRawDisplay,
    [property: JsonPropertyName("RADuration")] double RADuration,
    [property: JsonPropertyName("DECDistanceRaw")] double DECDistanceRaw,
    [property: JsonPropertyName("DECDistanceRawDisplay")] double DECDistanceRawDisplay,
    [property: JsonPropertyName("DECDuration")] double DECDuration,
    string Dither);
