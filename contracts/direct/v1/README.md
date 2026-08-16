# Direct protocol v1

Direct v1 carries N.I.N.A. identity, pairing/authentication, source-neutral
queries, typed commands, results, and heartbeats. JSON frames use the tagged
envelope described by `schema.json`.

The normative implementation is `src/direct/protocol.rs`. The schema and
fixtures are durable cross-repository compatibility inputs. Additive optional
fields may be introduced within v1; incompatible wire changes require a new
protocol directory and protocol version.
