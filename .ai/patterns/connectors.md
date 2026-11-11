# Pattern: a connector is data

How Devagotchi knows how to connect an AI client. Follow it for every new client.

1. **One file per client.** `crates/connectors/catalog/<vendor>/<id>.json`. The folder is the
   vendor, the file name is the `id`, and a test fails when they disagree.
2. **The schema comes from the types.** Change `crates/connectors/src/lib.rs`, then run
   `DEVAGOTCHI_WRITE_SCHEMA=1 cargo test -p devagotchi-connectors` to refresh
   `schemas/connector.v1.schema.json`. Never edit the schema by hand.
3. **No code path per client.** A difference between clients is a field in the data. If a client
   needs behavior the fields cannot express, extend the type for every client.
4. **Facts carry their source.** `verification` names the vendor's page and the date it was read.
   Write `null` when the vendor does not say, and put what is unconfirmed in `gaps`. Never fill
   a path or command in from memory.
5. **Proof has levels.** `docs` means it was read. `ci` means a real client connected in the
   named test. Only a test that ran promotes an entry.
6. **Placeholders are the only variables.** `{name}` and `{url}`, filled by the renderer.
7. **Every reader uses the renderer.** The `/connect` page, `GET /api/v1/connectors` and
   `devagotchi connect` show the same steps because they call the same function.
8. **Compiled in, never fetched.** Steps people run are reviewed in a pull request.

Add a client: write its file, add the line to `crates/connectors/src/catalog.rs`, and run
`cargo test -p devagotchi-connectors`. Its page, API entry, CLI steps and note aliases follow.
