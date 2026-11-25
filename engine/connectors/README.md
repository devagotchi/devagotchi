# devagotchi-connectors

How each AI client connects to a Devagotchi hub, as data. See the
[pattern](../../.ai/patterns/connectors.md) before adding one.

| Path | What it is |
|---|---|
| `catalog/<vendor>/<id>.json` | One client. The only place its facts live. |
| `src/lib.rs` | The types, the renderer, and catalog access. |
| `src/catalog.rs` | The files compiled in. A test holds it equal to the folder. |
| `src/tests.rs` | The catalog's rules, run as tests. |
| `../../schemas/connector.v1.schema.json` | Generated from the types. |

Readers: `/connect` and `GET /api/v1/connectors` on the hub, `devagotchi connect --list` and
`devagotchi connect ID --hub URL --print` on the command line, and the names a board note can be
addressed to.
