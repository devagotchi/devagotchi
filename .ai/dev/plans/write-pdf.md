# write_pdf: the built-in PDF writer (spec, P10)

Status: **proposed, waiting on hades's review. Nothing is built until it is approved.**
Issue: not_created. PR: not_created (no PR ceremony on this work, by the user's instruction).

Parent: [Desktop Commander parity](desktop-commander-parity.md), row P10. The gap list is
[.ai/docs/desktop-commander-parity.md](../../docs/desktop-commander-parity.md).

## 1. What Desktop Commander does

From the snapshot under `reference/DesktopCommanderMCP` (`src/server.ts`, `src/tools/schemas.ts`,
`package.json`):

- One tool, `write_pdf { path, content, outputPath?, options? }`.
- `content` as a string is markdown, turned into a PDF by `md-to-pdf`, which drives headless
  Chrome (Puppeteer). At startup Desktop Commander checks for Chrome and downloads it in the
  background if it is missing.
- `content` as an array is a list of page operations on an existing PDF, done with `pdf-lib`:
  - `delete { pageIndexes }`
  - `insert { pageIndex, markdown }`
  - `insert { pageIndex, sourcePdfPath }`
  A modified PDF must be written to a different `outputPath`.
- Page breaks use `<div style="page-break-before: always;"></div>`.
- Because Chrome renders it, any HTML, CSS and inline SVG works, and images can be given by
  path (`![alt](/path)` or `<img src>`).
- `options` is passed straight through to `md-to-pdf`.

Its cost: every machine needs a Chrome of its own, a download of over 100 MB, and a browser
runtime renders whatever an agent writes. (This snapshot has no `src/tools/pdf` files, so its
limits, warnings and bookmarks are not claimed either way.)

## 2. What Devagotchi has today

- File changes go through `crates/machine-engine/src/files/write.rs`:
  - The target resolves through links, with the write policy's reach: inside the workspace
    under `granted`, wherever the account can write under `account`.
  - A new file is written beside the target and renamed into place, keeping its permissions.
  - `expected_sha256` guards against a lost update.
  - At most `MAX_FILE_WRITE_BYTES` (32 KiB) per call.
- Every change takes the broker's mutation gate (P11, `broker/files.rs`): one at a time per
  broker, and `mutation_busy` after 3 s.
- The same request reaches the machine from `devagotchi mcp` (`crates/mcp/src/adapter/file_tools.rs`)
  and through a hub (`crates/hub/src/machines/files.rs`).
- No PDF, markdown, image or font crate is in `Cargo.lock` on main (a61cfb7).
- The broker builds on Linux and Windows (`machine-engine/Cargo.toml`). The toolchain is
  Rust 1.94 or later, edition 2024.
- Today's release archive is 10.4 MB for Linux x86_64 and 16.1 MB for Windows x86_64.

## 3. The library choice

**Recommendation: [Typst](https://typst.app) 0.15.1 for layout and PDF output, `lopdf` 0.47
for page edits on existing PDFs, and `pulldown-cmark` 0.13 for parsing markdown. All are pure
Rust, with no browser and no system library.**

### Measured, not assumed

A throwaway prototype was built on 2026-11-19 outside the repo, with Rust 1.97, `--release`
and `strip`. It turned a markdown file into Typst markup and compiled that with the embedded
fonts. The markdown had headings, bold, italic, strikethrough, inline code, a link, a GFM
table, nested and numbered lists, task lists, a fenced Rust block, a PNG, an SVG and a page
break. Then `lopdf` deleted a page.

| Measure | Result |
|---|---|
| Render | 2 pages, 37.9 KB, **41 ms** |
| Output checked | `pdftotext` read back every word; `pdftoppm` showed the table, the lists, the highlighted code, the PNG and the SVG |
| Page edit | `lopdf` deleted page 1: 2 pages became 1 |
| Renderer binary | 50.7 MB stripped, of which the embedded fonts are 9.7 MB; **14.9 MB** compressed with xz |
| Gaps found | CJK and emoji came out as boxes, because the embedded fonts don't cover them. See §6 for the fix |
| Licenses | 355 crates. All but three are already allowed in `deny.toml`. The three are BSD-2-Clause: `arrayref`, `kamadak-exif`, `mutate_once` |

### Why Typst

- It is a real typesetting engine: line breaking, hyphenation, tables, footnotes, math and
  code highlighting.
- It embeds and subsets its fonts.
- Its output is tagged PDF, which screen readers can read, and it builds bookmarks from the
  headings.
- PDF/A and PDF/UA are options. Leaving the timestamp out makes the output reproducible, so
  the same input gives the same SHA-256.
- It reads PNG, JPEG, GIF, WebP and SVG, and can place a page of another PDF as an image.
- It is sandboxed. It has no shell and no network, and it reads only the files that our
  `World` hands it.
- License: Apache-2.0.

### Rejected

| Option | Why not |
|---|---|
| Headless Chrome (Desktop Commander's way), or `wkhtmltopdf` | A browser on every machine, 100 MB or more. `wkhtmltopdf` is abandoned. This is the converter slot's job (§10), not the built-in's |
| `krilla` or `pdf-writer` alone | Good PDF output, but no layout: we would write our own line breaker and table engine. Typst already uses `krilla` underneath |
| `printpdf` 0.12 | Its HTML layout is young. Tables, lists and code would be ours to get right |
| `genpdf` | Unmaintained, built on an old `printpdf`, with weak tables |
| Typst through its CLI | Needs Typst installed on the machine. That is the converter path, not the built-in |

## 4. How it ships: a renderer of its own

**Recommendation:**
- PDF work runs in a separate binary, `devagotchi-pdf`, from a new crate `crates/pdf`. It
  ships next to `devagotchi` in the same release archive.
- The broker starts it as its own child process, with a deadline and a memory cap. It is not
  a user command, so it needs no exec grant and doesn't count as an operation.
- The broker talks to it in JSON over stdin and stdout.

Why a separate process:
1. **Size.** Linked into `devagotchi`, Typst would put about 40 MB into every hub, link and
   MCP process too. As its own file, it costs disk space only.
2. **Limits.** Typst has no overall time or memory limit of its own: deep recursion or a
   huge table runs until it is stopped. A child process can be killed at the deadline without touching the
   broker.
3. **Crashes.** A panic in layout ends the renderer, not the broker and its running jobs.

What it costs:
- The archive grows by about 15 MB compressed. It roughly doubles on Linux (10.4 MB to about
  25 MB).
- One process start per call. The prototype took 41 ms, so the start is not the slow part.

**Decision for hades (the only open question):** ship `devagotchi-pdf` in the release
archive (recommended), or download it on first use? The download keeps the archive at 10 MB.
But it needs network access the first time, and it adds an unsigned download path, because
signed updates are not built yet (README). Bundling has neither problem.

## 5. The tool contract

One tool, `write_pdf`, served the same way from `devagotchi mcp` and through a hub.
- The input is `PdfWriteInput` in `contracts/src/files.rs`. The answer is `PdfWritten`.
- The JSON schemas are regenerated (`scripts/generate-contracts.cjs`).

```
write_pdf {
  path:           string        // the PDF to write (create) or the PDF to change (edit)
  output_path?:   string        // where an edit is written; default: path itself (see below)
  markdown?:      string        // create: the document, up to 32 KiB inline
  source_path?:   string        // create: a markdown file on the machine, up to 8 MiB
  operations?:    Operation[]   // edit: applied in order to `path`
  page?:          { size?: "a4"|"letter"|"legal"|"a5"|..., landscape?: bool,
                    margin_mm?: number, font_size_pt?: number,
                    font?: "serif"|"sans"|"mono"|<installed font name>,
                    numbers?: bool, header?: string, footer?: string, title?: string,
                    toc?: bool }
  mode?:          "create" | "overwrite"   // default create: refuse if the target exists
  expected_sha256?: string      // the target's hash when you read it (lost-update guard)
  engine?:        "builtin"     // reserved for the converter slot (§10); only builtin now
  keep_source?:   bool          // also write <output>.typ beside the PDF, for debugging
}
Operation =
  { delete: { pages: number[] } }                                   // 1-based
  { insert: { at: number, markdown: string } }
  { insert: { at: number, pdf: string, pages?: string } }           // "1-3,7"
  { rotate: { pages: number[], degrees: 90|180|270 } }
  { reorder: { order: number[] } }
```

- Give exactly one of `markdown`, `source_path` or `operations`.
- Desktop Commander takes the same three inputs: its string, its array and its source PDF.

The answer:
- `path`, `pages`, `size`, `sha256` and `created`, as `write_file` gives today.
- `warnings`: one line for each thing not rendered as asked, with its markdown line. For
  example: an image not found, a glyph missing from every font, or HTML that was skipped.

Errors, each with its reason and nothing changed:
- `invalid`: bad input, or more than one of the three inputs.
- `path_not_found`
- `not_a_pdf`
- `pdf_encrypted`: `lopdf` cannot change an encrypted PDF.
- `page_out_of_range`
- `already_exists`
- `changed_since_read`
- `outside_workspace`
- `mutation_busy`
- `render_failed`: the Typst diagnostic, mapped back to the markdown line and column.
- `render_timeout`: past the deadline.
- `renderer_missing`: `devagotchi-pdf` isn't beside `devagotchi`; the message names the file.
- `engine_unavailable`: an engine other than `builtin`.

Editing in place:
- An edit may write over `path` itself, through the same new-file-and-rename as `write_file`.
- Desktop Commander refuses that and forces a new file name. Here the rename makes it safe,
  and `expected_sha256` guards it.
- `output_path` still writes elsewhere.

Units:
- Pages are 1-based, the way a person counts them. Desktop Commander's are 0-based.
- The schema says so in each field's description.

## 6. Coverage: what the built-in renders

| Feature | Built-in | Desktop Commander |
|---|---|---|
| Headings, paragraphs, bold, italic, strikethrough, links, block quotes, horizontal rules | yes (CommonMark plus GFM) | yes |
| Tables, with column alignment, long tables split across pages, headers repeated | yes | yes |
| Ordered, unordered, nested and task lists | yes | yes |
| Footnotes | yes, at the foot of the page | depends on `md-to-pdf`'s parser |
| Code blocks with syntax highlighting | yes (Typst's highlighter) | yes |
| Math (`$...$`, `$$...$$`) | yes, Typst math | not in its tool description |
| Images by path: PNG, JPEG, GIF, WebP, SVG | yes, read with the read policy, each up to 32 MiB | yes |
| Images by URL | no (planned later, through `read_url`'s fetcher) | yes, Chrome fetches them |
| Inline `<svg>...</svg>` blocks | yes, passed to Typst as an SVG image | yes |
| Page breaks | `<div style="page-break-before: always;"></div>` (its syntax, accepted as is), `<!-- pagebreak -->` and `\pagebreak` | the div |
| Page size, margins, landscape, page numbers, header and footer, title, contents | yes, through `page` | through `md-to-pdf` options |
| Bookmarks from headings, tagged PDF | yes | not in its tool description |
| Simple inline HTML: `<br>`, `<sub>`, `<sup>`, `<kbd>`, `<u>`, `<mark>`, `<span style="color:...">`, `<img src width>` | yes, mapped to Typst | yes |
| Any other HTML and CSS (flexbox, custom CSS, web fonts) | **no**: the text is kept, the tags are dropped, and a warning says so | yes |
| Advanced styling | a fenced ```` ```typst ```` block is passed through as Typst markup, in the same sandbox | HTML and CSS |
| Unicode, CJK, emoji | Latin, Greek and Cyrillic from the embedded fonts. CJK and emoji from the machine's own fonts when it has them; otherwise a warning names the missing script | whatever the machine's Chrome has |

Fonts:
- Embedded, so every machine renders the same: Libertinus Serif for text, New Computer
  Modern for math and DejaVu Sans Mono for code. They are OFL-1.1 and Bitstream Vera, noted
  in `LICENSE` third-party notices.
- The machine's own fonts are scanned once per renderer start (Typst's `scan-fonts`) and used
  as fallback and by name in `page.font`.

Where it falls short of Desktop Commander:
- Arbitrary HTML and CSS.
- Images by URL.

Both are named in the answer's warnings when used. The converter slot (§10) closes the first.

## 7. Reach and safety

- **Write:** the output path resolves exactly as `write_file`'s does: the same policy, the
  same gate, the same atomic replace and the same SHA guard.
- **Read:** markdown from `source_path`, images and inserted PDFs are read through the
  broker's read reach. The broker sends the renderer bytes, not paths.
  - The renderer gets no file system.
  - Its `World::file` answers only from the files the broker resolved and passed in.
- **No network** in the renderer.
- **Limits:**

  | What | Limit |
  |---|---|
  | Inline markdown | 32 KiB |
  | `source_path` | 8 MiB |
  | Each image | 32 MiB |
  | All inputs together | 128 MiB |
  | Pages out | 2,000 |
  | Output | 256 MiB |
  | Render deadline | 120 s, then `render_timeout` |
  | Renderer memory (Linux) | 1 GiB, set with `RLIMIT_AS` on the child |
  | Renderer memory (Windows) | the Job Object's memory limit |

- **Granted broker:** allowed, because it is a file write inside the workspace. The renderer
  is the broker's own child, not a granted program.

## 8. Tracing and debugging

- `broker.log` gets one line per call:
  - input kind, sizes, page count and duration in ms
  - each warning
  - on failure, the reason and the markdown line
  - renderer exit code and signal, for a crash or a kill
- `keep_source: true` writes the generated Typst next to the PDF. You can see exactly what
  was rendered, and run it with the Typst CLI if you have it.
- `render_failed` carries the Typst message, the markdown line and column, and a three-line
  excerpt.
- `devagotchi pdf render <in.md> <out.pdf> [--keep-source]` runs the same renderer from a
  shell, for debugging without an MCP client.
- Recent tool calls and usage stats (P9) count `write_pdf` like any other tool.

## 9. Where the code goes

| Place | What |
|---|---|
| `crates/pdf` (new; lib and bin `devagotchi-pdf`) | `markdown.rs` (pulldown-cmark to Typst, with escaping and the HTML subset), `world.rs` (the sandboxed Typst `World` and fonts), `edit.rs` (`lopdf` operations), `protocol.rs` (the stdin and stdout JSON), `main.rs` |
| `crates/contracts/src/files.rs` | `PdfWriteInput`, `PdfWritten`, `PdfOperation`, `PdfPage`, the limits, and the request method |
| `crates/machine-engine/src/files/pdf.rs` | resolve paths, read inputs, take the gate, run the renderer with deadline and memory cap, atomic replace |
| `crates/machine-engine/src/broker/files.rs` | route the new request through the mutation gate |
| `crates/mcp/src/adapter/file_tools.rs` | the local `write_pdf` tool |
| `crates/hub/src/machines/files.rs`, `mcp/` scope and popups, `tests/hub_probe.py` | the same tool through a hub |
| `dist-workspace.toml`, the installers, the auto-update | ship and update `devagotchi-pdf` with `devagotchi` |
| `deny.toml` | allow BSD-2-Clause |

Each `.rs` file stays at 500 lines or fewer, and the layout ratchet holds.

## 10. The converter slot (later, not in this build)

- `engine` is in the contract from day one. Only `builtin` is accepted now; any other value
  is refused with `engine_unavailable`.
- Later values:
  - `pandoc`: markdown to PDF with the machine's pandoc, and the PDF engine it finds.
  - `libreoffice`: `soffice --headless --convert-to pdf` for DOCX, ODT, PPTX, XLSX and HTML
    input.
  - `chrome`: a headless Chrome the machine already has, for full HTML and CSS. This is
    Desktop Commander's way, but only where Chrome is already installed.
  - `auto`: the best one installed, falling back to `builtin`.
- Each runs as the broker's child, under the same deadline, limits, gate, atomic replace and
  answer. Only the renderer changes.
- `broker_capabilities` gains `pdf_engines`, the list found on the machine, so an agent knows
  before it asks.
- The same slot then covers the Office-format gaps in the parity doc (reading and writing
  DOCX and XLSX). That is the decision the earlier "built in, converter, or both" question
  was about. This spec makes the built-in the default and the fallback, and keeps the slot
  open.

## 11. Milestones and acceptance

| Step | What | Accepted when |
|---|---|---|
| P10a | `crates/pdf`: markdown to Typst, the sandboxed World, fonts with system fallback, the CLI | Golden tests for each row of §6, checked by text extraction (`lopdf` text and `pdftotext` where present). A CJK sample renders with a system Noto CJK font, and without one it gives the warning. A runaway document (deep recursion, a huge table) is killed at the deadline. Escaping: `#`, `$`, `<`, `@`, `\` and `*` in text come out literal |
| P10b | page edits with `lopdf` | delete, insert markdown, insert from a PDF with a range, rotate and reorder, each checked by page count and text. An encrypted PDF gives `pdf_encrypted` |
| P10c | the broker, `devagotchi mcp`, the gate, limits and tracing | probes through `devagotchi mcp`: create, edit in place with a SHA, `already_exists`, `changed_since_read`, `outside_workspace` on a granted broker, `renderer_missing` with the binary moved away; `broker.log` shows each |
| P10d | through the hub and the e2e | `hub_probe.py` lists `write_pdf`; e2e writes a PDF on the test machine through the hub and reads its page count back; the parity doc's row turns to present |
| P10e | shipping | the release archive holds `devagotchi-pdf` on Linux and Windows; auto-update replaces both |

There is no CI work until the working alpha, as the user said. Each step lands on main when
it works.

## 12. Open question

1. Ship `devagotchi-pdf` in the release archive (recommended), or download it on first use?
   See §4.

Everything else here is a recommendation that hades can change on review.
