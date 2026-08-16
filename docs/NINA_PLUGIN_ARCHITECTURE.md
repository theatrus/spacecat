# N.I.N.A. plugin architecture

The Chatstronomy N.I.N.A. plugin is the only observatory data source. It reads
public N.I.N.A. mediators and bounded native histories in-process, then answers
typed Direct queries from either a local bundled runtime or the hosted Hub.

## Data flow

```text
N.I.N.A. mediators / sequence / notifications / log
                         |
                  Chatstronomy plugin
                    /            \
       local named pipe        TLS WebSocket
              |                     |
     bundled local runtime      Chatstronomy Hub
              |                     |
       Discord / Matrix        Discord / commands
```

Local Direct mode uses a random current-user-only bootstrap pipe for secrets and
a node-scoped data pipe for typed queries. The child runtime starts and stops
with the plugin by default. No TCP listener, pairing token, or observatory URL is
required.

Hosted mode connects from the plugin to `/v1/direct` on the Hub. A one-time
pairing token becomes a profile-and-node-bound credential stored with Windows
Credential Manager. Multiple profiles and systems can connect concurrently.

## Source contract

`RigSource` is the transport-neutral boundary used by status polling, chart
rendering, Discord slash commands, and Matrix/Discord delivery. It includes:

- bounded event, image, and sequence snapshots;
- thumbnails, autofocus data, guider data and rendered graph inputs;
- mount, camera, filter wheel, guider, rotator, and focuser snapshots;
- typed commands such as park, guide, cool, autofocus, and sequence control.

Direct v1 envelopes currently advertise additive payload contract v2. Older
plugin payloads without `ChatEnabled` remain accepted and default to delivery
enabled. The server labels unmarked payloads as legacy Direct v1; this is Direct
protocol compatibility, not a second data-source mode.

## Event delivery and state

The plugin attaches `ChatEnabled` to captured events, images, targets, and
long-running sequence operations. The Rust updater always consumes those values
for state reconstruction and deduplication, but posts only values enabled by the
profile. This allows a user to suppress noisy messages without breaking target,
wait, cooling, guider, or sequence state.

Target Scheduler integration follows its N.I.N.A. message-broker topics and
projects the active container's `Target.TargetName`, avoiding the generic
“Sequential Instruction Set” wrapper name.

Popup notifications are observed from N.I.N.A.'s toast lifetime supervisor.
Raw N.I.N.A. logs are tailed from the active process log and can be enabled by
level. Log delivery is opt-in due to volume and possible private path/device
content.

## Security boundaries

- Local secrets are sent only over a current-user named pipe and are not placed
  in arguments, environment variables, or generated configuration files.
- Hosted connections are outbound TLS WebSockets.
- Matrix homeserver URLs must use HTTPS.
- Hub commands are typed, expire, and are authorized against telescope routing
  and guild policy before reaching N.I.N.A.
- Direct histories are bounded to prevent unbounded plugin memory growth.
