# Direct protocol v1

Direct v1 carries N.I.N.A. identity, pairing/authentication, source-neutral
queries, typed commands, results, and heartbeats. JSON frames use the tagged
envelope described by `schema.json`.

The normative implementation is `src/direct/protocol.rs`. The schema and
fixtures are durable cross-repository compatibility inputs. Additive optional
fields may be introduced within v1; incompatible wire changes require a new
protocol directory and protocol version.

`payload_version` marks the additive data contract independently of the Direct
envelope. Current clients advertise payload version 2. A Direct v1 hello that
omits the field is an explicitly supported legacy payload-version-1 client;
the Hub echoes version 1 in its agent hello and keeps accepting its original
frames. `fixtures/client-hello-legacy.json` is the frozen unmarked legacy form.
