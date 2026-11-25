# PDF in Devagotchi: write_pdf, read_pdf and the shared font pack (spec, P10)

Status: **proposed, revision 2, waiting on hades's review. Nothing is built until it is
approved.**
Issue: not_created. PR: not_created (no PR ceremony on this work, by the user's instruction).

Parent: [Desktop Commander parity](desktop-commander-parity.md), row P10. The gap list is
[.ai/docs/desktop-commander-parity.md](../../docs/desktop-commander-parity.md).
Sibling: [dev build on the phone, dev stats and diagnostics export](dev-build-and-diagnostics.md),
whose tracing (request ids, JSON lines) this spec's logging uses.

Revision 2 (2026-11-19) answers hades's review of revision 1: "optimize this first and have it
provide all necessary pdf common capabilities", then "if the size is the font we should keep
but also benefit from having those in the app", built the way a cloud engineer would for
their own self-hosted setup. What changed:
- The size was measured properly (§4). It is the engine, not the fonts.
- The engine links into `devagotchi` and runs as a child of itself, so there is no second
  program to ship, and the bundle-or-download question of revision 1 goes away.
- Fonts move out of the binary into a shared font pack that the whole app uses, the UI
  included (§5).
- The common PDF jobs are inventoried with crate and license (§6), and the tools grow to
  cover them (§7).
- Self-hosting gets its packaging, paths, config keys and optional converters spelled out
  (§10).

## 1. What Desktop Commander does

From the snapshot under `reference/DesktopCommanderMCP` (`src/server.ts`, `src/tools/schemas.ts`,
`package.json`):

- One tool, `write_pdf { path, content, outputPath?, options? }`.
- `content` as a string is markdown, turned into a PDF by `md-to-pdf`, which drives headless
  Chrome (Puppeteer). At startup Desktop Commander checks for Chrome and downloads it in the
  background if it is missing.
- `content` as an array is a list of page operations on an existing PDF, done with `pdf-lib`:
  `delete { pageIndexes }`, `insert { pageIndex, markdown }`, `insert { pageIndex,
  sourcePdfPath }`. A modified PDF must be written to a different `outputPath`.
- Page breaks use `<div style="page-break-before: always;"></div>`.
- Because Chrome renders it, any HTML, CSS and inline SVG works, and images can be given by
  path (`![alt](/path)` or `<img src>`).
- `options` is passed straight through to `md-to-pdf`.

Its cost: every machine needs a Chrome of its own, a download of over 100 MB, and a browser
runtime renders whatever an agent writes.

## 2. What Devagotchi has today

- File changes go through `engine/machine-engine/src/files/write.rs`: the write policy's
  reach, a new file written beside the target and renamed into place, `expected_sha256`
  against lost updates, at most 32 KiB per call (`MAX_FILE_WRITE_BYTES`).
- Every change takes the broker's mutation gate (P11, `broker/files.rs`).
- The same request reaches the machine from `devagotchi mcp` and through a hub.
- No PDF, markdown, image or font crate is in `Cargo.lock` on main.
- One binary, `devagotchi`, is the hub, the link, the broker and `devagotchi mcp`. It is
  33.0 MB built with the `dist` profile (fat LTO, one codegen unit, symbols stripped), 9.7 MB
  compressed with xz. The release archive is 10.4 MB for Linux x86_64 and 16.1 MB for
  Windows x86_64.
- The hub's UI has a font slot (`tokens.json` `font_files`, `engine/hub/src/site/style.rs`)
  that is wired but empty: today the app uses each device's system fonts.

## 3. The library choice

**Typst 0.15.1 for layout and PDF output, `pulldown-cmark` 0.13 for markdown, `lopdf` 0.47
for changing and reading existing PDFs, and `hayro` 0.7 for turning pages into images. All
are pure Rust, with no browser and no system library. `hayro` comes free: Typst already
depends on it.**

Why Typst:
- A real typesetting engine: line breaking, hyphenation, tables, footnotes, math and code
  highlighting.
- It embeds and subsets fonts, so a 17 MB CJK font adds only the glyphs used.
- Tagged PDF by default (screen readers), bookmarks from headings, PDF/A and PDF/UA as
  options, reproducible output when the timestamp is left out.
- It reads PNG, JPEG, GIF, WebP and SVG, and can place a page of another PDF as an image.
- Sandboxed: no shell and no network, and it reads only the files our `World` hands it.
- License: Apache-2.0.

Rejected:

| Option | Why not |
|---|---|
| Headless Chrome or `wkhtmltopdf` | A browser on every machine, 100 MB or more; `wkhtmltopdf` is abandoned. It is the converter slot's job (§11) |
| `krilla` or `pdf-writer` alone | Good PDF output, but no layout. Typst uses `krilla` underneath already |
| `printpdf` 0.12, `genpdf` | Young or unmaintained layout; tables, lists and code would be ours to get right |
| `mupdf` bindings | AGPL-3.0: copyleft for the whole program. Not linked, ever |
| `pdfium-render` | MIT or Apache, but it needs Google's native pdfium library on every machine and for every target |
| `qpdf` bindings | A native C++ library. Offered through the converter slot instead (§11) |

## 4. Optimizing: measured, not assumed

Measured on 2026-11-19 in a 4-core cloud container, Rust 1.97, with a throwaway prototype
outside the repo, and with that prototype linked into a throwaway copy of `devagotchi` built
exactly as releases are (`--profile dist`). MB is a million bytes; "xz" is `xz -9`.

### Size

| Build | Binary | Compressed (xz) |
|---|---|---|
| `devagotchi` today | 33.0 MB | 9.7 MB |
| `devagotchi` with the PDF engine linked in (Typst, `lopdf`, `hayro`, `pulldown-cmark`, plus a 3.1 MB font set embedded for the test) | 70.4 MB | 21.5 MB |
| The same engine as a separate program (revision 1's plan), `dist` profile | 40.7 MB | 13.0 MB |
| That separate program built for size (`opt-level = "s"`) | 34.7 MB | 11.4 MB |

| Fonts | Files | Compressed (xz) |
|---|---|---|
| Typst's full set (Libertinus Serif, New Computer Modern, NewCM Math, DejaVu Sans Mono, and the Foxit standard fonts `hayro` uses to read PDFs) | 9.9 MB | 4.3 MB |
| A minimal set (7 files) | 3.1 MB | 1.6 MB |
| A CJK font (WenQuanYi Zen Hei, one file) | 16.8 MB | 7.5 MB |
| Noto Color Emoji | 10.8 MB | 9.7 MB |

What the numbers say:
1. **The size is the engine, not the fonts.** Linked in, the engine adds about 34 MB to the
   binary and about 10 MB to the download, after taking out the test fonts. The whole font
   set is 4.3 MB of download. So the fonts stay, in full, and go in the pack (§5).
2. **Linking in costs less than a second program.** The two share libraries (`image`,
   `serde`, compression, hashing), so one binary is 1.2 MB smaller to download than two.
3. **Building the engine for size saves 1.6 MB of download with no measurable slowdown**
   (below). This can be applied to the Typst crates alone with per-package `opt-level`
   overrides in `[profile.dist]`, leaving the rest of `devagotchi` at full speed. P10a
   measures it inside `devagotchi` before adopting it.
4. Expected release archive for Linux x86_64: 10.4 MB today, about 21 MB with the engine,
   about 25 MB with the full font pack. Built from the measured parts; P10f measures the
   real archive.

### Speed

| Job | Time | Peak memory |
|---|---|---|
| Render a 2-page sample (every markdown feature, a PNG, an SVG) | 36 to 59 ms | 35 MB |
| Render 80 pages (114 KB of markdown) | 120 to 135 ms | 113 MB |
| The same again in the same process (Typst caches) | 4 to 13 ms for 2 pages | |
| Whole `devagotchi` process: start, render 80 pages, rasterize page 1, parse the PDF | 274 ms | 81 MB |
| Page 1 to PNG at 144 dpi (`hayro`) | 28 ms | 22 MB |
| Extract the text of 80 pages (`lopdf`) | 41 ms | 19 MB |
| Delete a page, compress and save (`lopdf`) | 50 ms (2-page sample: 38 KB to 20 KB) | 25 MB |
| Load a CJK font and the emoji font from disk at start | 23 ms | |
| `opt-level = "s"` build, 80 pages | 120 to 135 ms (same as full speed) | |

Checked, not just timed:
- `pdftotext` read back every word, and page images showed the table, lists, highlighted
  code, the PNG and the SVG.
- With the CJK and emoji fonts loaded from disk, "中文" and "🙂" rendered. The PDF embedded
  only the subsets: 49 KB for the 2 pages.
- The encrypted output opened with its password in `pdfinfo` and `pdftotext`. That test used
  RC4-128. `lopdf` also does AES-256 (V5), which the tool will use; P10b tests it.
- One defect to fix: after `lopdf` saved, `pdfinfo` warned "Suspects object is wrong type".
  P10b's acceptance requires `qpdf --check` and `pdfinfo` to be clean.

### How it ships

**Recommendation:**
- The engine is a new crate, `crates/pdf`, linked into `devagotchi` behind a cargo feature,
  `pdf`, which is on by default.
- The broker runs the PDF work in a child of itself: `devagotchi pdf-worker` (hidden), with
  a deadline and a memory cap, speaking JSON on stdin and stdout.
- The binary embeds no fonts. They come from the font pack (§5).

Why a child of itself:
- One file to ship, sign and update, so there is no "renderer missing" state and nothing to
  download separately.
- Still isolated. Typst has no overall time or memory limit of its own. A child is killed at
  the deadline, and a panic in layout ends the child, not the broker and its running jobs.
- The start is cheap: the whole process above took 274 ms for 80 pages.
- A self-hoster who builds from source can leave it out with `--no-default-features`. Then
  `write_pdf` and `read_pdf` are not listed, and a call answers `engine_unavailable`.

The cost: the binary is about 34 MB bigger on disk for every role, the hub's included. Code
that a process never runs is mapped but mostly never read from disk, so the hub, link and
`mcp` processes grow little in memory. P10a measures their resident memory before and after.

## 5. The shared font pack

The fonts become part of the app, not a private detail of the PDF engine.

### What is in it

| Pack | Files | Who uses them | Download |
|---|---|---|---|
| `core` (always in the release) | Libertinus Serif (6 styles), New Computer Modern (4), NewCM Math (3), DejaVu Sans Mono (4), the Foxit standard fonts (14) | PDF text, math and code; `hayro`'s stand-ins for the 14 standard PDF fonts; the app's code and log views; document previews | 4.3 MB |
| `ui` (if hades chooses it, §14) | One sans family for the app's text | The PWA and the desktop app | measured at P10f |
| `cjk`, `emoji` (extras) | A CJK font; Noto Color Emoji | PDFs, and the app on devices that lack them | 7.5 MB and 9.7 MB |

Each file's license is kept with it, under `LICENSES/`, and listed in `fonts.json`.
Libertinus is OFL-1.1 and DejaVu is the Bitstream Vera license; the rest are confirmed per
file at P10f before they ship.

### How it is laid out

```
share/devagotchi/fonts/
  fonts.json                // one entry per file: family, style, roles, license, sha256, pack
  core/*.otf *.ttf *.pfb    // copied from the typst-assets crate pinned in Cargo.lock
  web/*.woff2               // the same faces for browsers, made when the release is staged
  LICENSES/
```

- `fonts.json` is the one list everything reads. The PDF engine reads the font files. The
  hub reads `web/` and serves it.
- Roles in `fonts.json` say what each face is for: `text`, `math`, `mono`, `ui`,
  `pdf-standard`. The PDF engine picks its defaults by role, and the hub fills `tokens.json`
  `font_files` from the same list. That keeps the app and the PDFs in step.
- The `woff2` files are made from the same masters when the release is staged
  (`scripts/stage-fonts.sh`, called by `scripts/stage-downloads.sh`). `.otf`
  stays the source; `style.rs` keeps accepting only `.woff` and `.woff2`.

### How it is bundled and downloaded

- **Release archives** (cargo-dist, all four targets) carry `share/devagotchi/fonts/` with
  `core` (and `ui` if chosen) next to the binary.
- **Installers** put it inside each version's directory, beside the binary (§10), so the
  binary and its pack always switch and roll back together.
- **Machine self-update** (`runtime/link/src/update/mod.rs`) replaces the pack with the binary.
  Every font file is listed in the release's `SHA256SUMS` and checked the same way the
  binary is.
- **The hub's Docker image** carries the pack at `/usr/share/devagotchi/fonts`. The hub
  serves `web/` at `/fonts/<file>`, with a content hash in the name and an immutable cache
  header. The service worker caches them, so the installed phone app has its fonts offline.
- **Extras** (`cjk`, `emoji`) are not in the archive (unless hades chooses so, §14). The hub
  stages them under `/dist/fonts/` with the other downloads. A machine fetches one when a
  render needs a script it can't draw, or on `devagotchi fonts add cjk`. It is checked
  against `SHA256SUMS`, like a machine update. Until then the render still succeeds, and its
  warning names the missing script and the command to add it.
- **Any font a self-hoster drops in** a fonts directory (§10) is picked up, `.ttf`, `.otf`
  and `.ttc`. The machine's own system fonts are scanned too, unless switched off.

## 6. Common PDF jobs: coverage, crate and license

What PDF tools usually do (Desktop Commander, `pdftk`, `qpdf`, PyPDF and similar), and what
does each here. "v1" is in this build; "later" is planned after it; "slot" goes through the
converter slot (§11), using a program the machine already has; "out" is not planned.

| Job | Plan | By | License | Notes |
|---|---|---|---|---|
| Create from markdown | v1 | Typst, `pulldown-cmark` | Apache-2.0, MIT | measured |
| Create from HTML/CSS, DOCX, ODT, PPTX, XLSX | slot | Chrome, LibreOffice | (native programs) | not linked |
| Extract text | v1 | `lopdf` | MIT | measured; P10c compares layout order with `pdf-extract` (MIT) and keeps the better one |
| Extract images | v1 | `lopdf`, `image` | MIT, MIT/Apache | JPEG copied as is, others re-encoded as PNG |
| Page to image (PNG) | v1 | `hayro` | Apache-2.0 or MIT | measured; already in Typst's tree |
| Merge | v1 | `lopdf` | MIT | |
| Split, extract pages | v1 | `lopdf` | MIT | |
| Delete, insert, rotate, reorder pages | v1 | `lopdf` | MIT | delete measured |
| Read and set metadata (Info and XMP) | v1 | `lopdf` | MIT | |
| Bookmarks (outline) | v1 | Typst from headings; `lopdf` on existing PDFs | Apache-2.0, MIT | |
| Watermark, stamp, page numbers on an existing PDF | v1 | Typst draws the overlay; `lopdf` lays it over each page as a form XObject | Apache-2.0, MIT | |
| Fill form fields | v1 | `lopdf` | MIT | sets values and asks viewers to redraw them |
| Flatten forms | later | own code on `lopdf`, checked with `hayro` | MIT | needs appearance streams drawn; `pdf_form` (MIT) is unmaintained since 2020 |
| Encrypt, decrypt with a password | v1 | `lopdf` | MIT | AES-256 for writing; older RC4 files can be read |
| Compress, optimize | v1 | `lopdf` | MIT | object streams, unused objects dropped; measured. Image downsampling is later |
| Tagged PDF, PDF/A, PDF/UA when creating | v1 | Typst | Apache-2.0 | |
| Convert an existing PDF to PDF/A | out | | | |
| Linearize (fast web view) | slot | `qpdf` | Apache-2.0, native | |
| Repair a damaged PDF | v1 partial, slot full | `lopdf`'s lenient reader; `qpdf` | MIT; native | |
| OCR a scanned PDF | slot | `ocrmypdf` with Tesseract | MPL-2.0 and Apache-2.0, native | |
| Digital signatures, sign and verify | later | RustCrypto `cms` with `lopdf` | Apache-2.0 or MIT | needs its own design: keys, timestamps, trust |
| Redaction that truly removes content | out | | | needs content-stream rewriting; the libraries that do it are `mupdf` (AGPL) and pdfium (native), both rejected |
| Compare two PDFs | out | | | text diff of extracted text is possible later |
| Add annotations or comments | later | `lopdf` | MIT | |

Copyleft or native dependencies, flagged:
- `mupdf` (AGPL-3.0, copyleft): rejected, never linked.
- pdfium (native C++): rejected.
- `qpdf`, LibreOffice, Chrome, Tesseract, poppler (native): only through the converter slot,
  and only when already installed on the machine. Never linked or shipped by us. Poppler is
  GPL, so it stays a separate program for the same reason.
- New licenses for `deny.toml`: BSD-2-Clause (three crates in Typst's tree: `arrayref`,
  `kamadak-exif`, `mutate_once`). Everything else is already allowed.

## 7. The tool contracts

Two tools, served the same way from `devagotchi mcp` and through a hub. Inputs and answers
live in `contracts/src/files.rs`, and the JSON schemas are regenerated
(`scripts/generate-contracts.cjs`). Pages are 1-based, as a person counts them (Desktop
Commander's are 0-based), and each field's description says so.

### `write_pdf`: create, or change an existing PDF

```
write_pdf {
  path:             string        // the PDF to create, or the PDF to change
  output_path?:     string        // where a change is written; default: path itself
  markdown?:        string        // create: the document, up to 32 KiB inline
  source_path?:     string        // create: a markdown file on the machine, up to 8 MiB
  operations?:      Operation[]   // change: applied in order
  page?:            { size?, landscape?, margin_mm?, font_size_pt?, font?, numbers?,
                      header?, footer?, title?, toc? }
  standard?:        "pdf-a-2b" | "pdf-ua-1"
  mode?:            "create" | "overwrite"     // default create: refuse if it exists
  expected_sha256?: string        // the target's hash when you read it
  password?:        string        // to open an encrypted `path`
  engine?:          "builtin"     // the converter slot's other values come later (§11)
  keep_source?:     bool          // also write <output>.typ, for debugging
}
Operation =
  { delete:   { pages: number[] } }
  { insert:   { at: number, markdown: string } }
  { insert:   { at: number, pdf: string, pages?: "1-3,7" } }
  { rotate:   { pages: number[], degrees: 90 | 180 | 270 } }
  { reorder:  { order: number[] } }
  { merge:    { pdfs: string[] } }                       // append, in order
  { split:    { into: string, ranges?: string[], every?: number } }   // into a directory
  { metadata: { title?, author?, subject?, keywords?, creator? } }
  { outline:  { entries: [{ title, page, level }] } }   // replaces the bookmarks
  { stamp:    { markdown: string, pages?: "1-", position?: "center" | "top" | "bottom",
                opacity?: number, behind?: bool } }      // watermarks and page numbers
  { fill:     { fields: { [name]: string | bool } } }
  { encrypt:  { user_password?: string, owner_password: string,
                allow?: ("print" | "copy" | "edit" | "annotate" | "fill")[] } }   // AES-256
  { decrypt:  {} }                                       // with `password`
  { compress: {} }
```

- Give exactly one of `markdown`, `source_path` or `operations`. Desktop Commander takes the
  same three inputs.
- A change may write over `path` through the same new-file-and-rename as `write_file`, guarded
  by `expected_sha256`. Desktop Commander refuses that.
- `split` writes several files into a directory, each through the same atomic write.
- The answer: `path`, `pages`, `size`, `sha256` and `created` (as `write_file` gives), the
  list of files for `split`, and `warnings`, one line for each thing not done as asked, with
  its markdown line.

### `read_pdf`: what is inside a PDF

```
read_pdf {
  path:      string
  password?: string
  pages?:    "1-3,7"                 // default: all, within the limits
  what:      "text" | "info" | "outline" | "fields" | "images" | "page_image"
  dpi?:      number                  // page_image, default 144, at most 300
  into?:     string                  // images: a directory to write them to
}
```

- `text`: the text per page, up to the read limit (24 KiB per call, paged like `read_file`).
- `info`: page count, page sizes, metadata, whether it is encrypted, tagged or PDF/A, and
  its fonts.
- `outline`: the bookmarks. `fields`: form field names, types and values.
- `images`: a list with sizes, and written to `into` when it is given.
- `page_image`: one page as a PNG, answered as an image the way `read_image` answers.
- `read_file` on a `.pdf` keeps its current behavior, and its message points to `read_pdf`.

### Errors, each with its reason and nothing changed

`invalid`, `path_not_found`, `not_a_pdf`, `pdf_encrypted` (no or wrong `password`),
`page_out_of_range`, `field_not_found`, `already_exists`, `changed_since_read`,
`outside_workspace`, `mutation_busy`, `render_failed` (the Typst message mapped back to the
markdown line and column), `render_timeout`, `worker_failed` (the child crashed or ran out of
memory, with its exit code), `engine_unavailable`.

## 8. What the built-in renders from markdown

| Feature | Built-in | Desktop Commander |
|---|---|---|
| Headings, paragraphs, bold, italic, strikethrough, links, block quotes, rules | yes (CommonMark plus GFM) | yes |
| Tables, with alignment, split across pages, headers repeated | yes | yes |
| Ordered, unordered, nested and task lists | yes | yes |
| Footnotes | yes | depends on `md-to-pdf` |
| Code blocks with syntax highlighting | yes | yes |
| Math (`$...$`, `$$...$$`) | yes | not in its tool description |
| Images by path: PNG, JPEG, GIF, WebP, SVG | yes, through the read policy, each up to 32 MiB | yes |
| Images by URL | later, through `read_url`'s fetcher | yes |
| Inline `<svg>` | yes | yes |
| Page breaks | its `page-break-before` div as is, `<!-- pagebreak -->`, `\pagebreak` | the div |
| Page size, margins, landscape, numbers, header, footer, title, contents | yes, through `page` | through `md-to-pdf` options |
| Bookmarks, tagged PDF, PDF/A | yes | not in its tool description |
| Simple inline HTML (`<br>`, `<sub>`, `<sup>`, `<kbd>`, `<u>`, `<mark>`, colored `<span>`, `<img width>`) | yes | yes |
| Any other HTML and CSS | no: text kept, tags dropped, a warning says so; the `chrome` converter covers it (§11) | yes |
| Advanced styling | a fenced ```` ```typst ```` block, in the same sandbox | HTML and CSS |
| Unicode, CJK, emoji | Latin, Greek and Cyrillic from `core`; CJK and emoji from the extras or the machine's own fonts; otherwise a warning with the command to add them | whatever the machine's Chrome has |

## 9. Reach, safety and limits

- **Write:** output paths resolve exactly as `write_file`'s do: the same policy, the same
  gate, the same atomic replace and the same SHA guard. `read_pdf` uses the read policy.
- **Read:** markdown, images and other PDFs are read by the broker, through its read reach.
  The worker gets bytes, not paths, and its `World` answers only from what it was handed.
- **No network** in the worker. Fonts are read by the broker from the font directories (§10)
  and handed over too.
- **Passwords** given to `write_pdf` or `read_pdf` never reach a log line, a trace or a
  diagnostics bundle. They are dropped from the logged input, and the redaction rules in the
  diagnostics spec match them too.
- **Granted broker:** allowed, because these are file reads and writes inside the workspace.
  The worker is the broker's own child, not a granted program.
- **Limits:**

  | What | Default | Config key (§10) |
  |---|---|---|
  | Inline markdown | 32 KiB | |
  | `source_path` | 8 MiB | |
  | Each image or inserted PDF | 32 MiB | |
  | All inputs together | 128 MiB | |
  | Pages out | 2,000 | `pdf.max_pages` |
  | Output | 256 MiB | |
  | Deadline | 120 s, then `render_timeout` | `pdf.deadline_seconds` |
  | Worker memory | 1 GiB (`RLIMIT_AS` on Linux, a Job Object on Windows) | `pdf.memory_mib` |

## 10. Self-hosted setup

### What is installed where

| Install | Binary | Font pack |
|---|---|---|
| Release archive (Linux, Windows) | `devagotchi` | `share/devagotchi/fonts/` beside it |
| `install-devagotchi.sh` (default prefix `/opt/devagotchi`) | `<prefix>/versions/<version>/devagotchi`, with `<prefix>/current` pointing at it | `<prefix>/versions/<version>/share/devagotchi/fonts/`, so a rollback brings back the matching pack |
| `install-devagotchi.ps1` (its `-Root`) | `<root>\versions\<version>\devagotchi.exe`, with `<root>\current` | `<root>\versions\<version>\share\devagotchi\fonts\` |
| Hub Docker image | `/usr/local/bin/devagotchi` | `/usr/share/devagotchi/fonts/` |
| Built from source | `target/dist/devagotchi` | `scripts/stage-fonts.sh` copies the pack from the `typst-assets` crate pinned in `Cargo.lock` into `target/fonts/`, so no font files are committed; found through `DEVAGOTCHI_FONTS_DIR` or the config |

The font pack is found in this order, first match wins for each file:
1. `pdf.fonts_dirs` in `broker.json` (several directories allowed).
2. `DEVAGOTCHI_FONTS_DIR`.
3. `share/devagotchi/fonts` beside the binary, then `../share/devagotchi/fonts`.
4. `/usr/share/devagotchi/fonts` (Linux).
5. The machine's system fonts, unless `pdf.system_fonts` is false.

The extras land in the first writable directory of that list, or in the state directory.

### Broker settings (`broker.json`)

```json
"pdf": {
  "enabled": true,
  "fonts_dirs": [],
  "system_fonts": true,
  "deadline_seconds": 120,
  "memory_mib": 1024,
  "max_pages": 2000,
  "engines": {
    "pandoc": null, "libreoffice": null, "chrome": null, "qpdf": null, "ocrmypdf": null
  }
}
```

- The whole `pdf` object is optional, and every key has the default shown, so today's
  `broker.json` files keep loading (the config rejects unknown fields, so the new object is
  added to `BrokerConfig` with defaults).
- `engines`: `null` looks for the program on `PATH`, a path uses that file, and `false` never
  uses it.
- `deadline_seconds`, `memory_mib` and `max_pages` change at runtime through
  `set_config_value`, like today's limits. The others take effect at the next start.
- `enabled: false` switches PDF tools off on that machine without a rebuild.

### Hub settings (environment)

| Variable | Default | What |
|---|---|---|
| `DEVAGOTCHI_FONTS_DIR` | `/usr/share/devagotchi/fonts` in the image | where the pack is |
| `DEVAGOTCHI_HUB_FONTS` | `serve` | `serve` serves the pack to the app at `/fonts/`; `system` keeps device fonts |

The hub doesn't render PDFs, so its image needs no converter. Machines do the work.

### Optional converters, per platform

None is required. Each one adds the jobs marked "slot" in §6.

| Converter | Debian, Ubuntu | Fedora | Windows (winget) | Adds |
|---|---|---|---|---|
| `qpdf` | `qpdf` | `qpdf` | `QPDF.QPDF` | linearize, full repair |
| `ocrmypdf` | `ocrmypdf` | `ocrmypdf` | (WSL) | OCR |
| LibreOffice | `libreoffice-core-nogui` | `libreoffice-core` | `TheDocumentFoundation.LibreOffice` | DOCX, ODT, PPTX, XLSX to PDF |
| Pandoc | `pandoc` | `pandoc` | `JohnMacFarlane.Pandoc` | other markup to PDF |
| Chrome or Chromium | `chromium` | `chromium` | `Google.Chrome` | full HTML and CSS |

Package names are checked against each distribution at P10g before the docs ship.

### Checking a machine

- `devagotchi diag check` (from the diagnostics spec) prints: the build; whether PDF is
  built in and enabled; where the font pack was found, its files and any that fail their
  checksum; the extras present; and each converter found, with its version.
- `broker_capabilities` gains `pdf` (enabled, packs present) and `pdf_engines` (converters
  found), so an agent knows before it asks.

## 11. The converter slot (after v1)

- `engine` is in the contract from day one. Only `builtin` is accepted in v1; any other value
  answers `engine_unavailable`.
- Later values: `pandoc`, `libreoffice`, `chrome`, `qpdf` and `ocrmypdf` for the jobs marked
  "slot" in §6, and `auto`, the best one installed, falling back to `builtin`.
- Each runs as the broker's child under the same deadline, limits, gate, atomic replace and
  answer. Only the program changes.
- The same slot covers the Office-format gaps in the parity doc (reading and writing DOCX
  and XLSX).

## 12. Tracing and debugging

- Every call logs one line through the tracing in the diagnostics spec: request id, tool,
  operation kinds, input sizes, pages, milliseconds, warnings, and on failure the reason and
  markdown line. The worker's exit code and signal are logged for a crash or a kill.
- `keep_source: true` writes the generated Typst beside the PDF.
- `render_failed` carries the Typst message, the markdown line and column, and a three-line
  excerpt.
- From a shell: `devagotchi pdf render <in.md> <out.pdf> [--keep-source]`,
  `devagotchi pdf read <file.pdf> [--what text|info|...]`, `devagotchi pdf edit <file.pdf>
  <ops.json>`, and `devagotchi fonts list|add`. They run the same worker.
- Recent tool calls and usage stats count both tools like any other.

## 13. Where the code goes, and milestones

| Place | What |
|---|---|
| `crates/pdf` (new, library) | `markdown.rs` (to Typst, escaping, the HTML subset), `world.rs` (the sandboxed `World`), `fonts.rs` (the pack and `fonts.json`, also used by the hub), `edit.rs` and `stamp.rs` (`lopdf` operations), `read.rs` (text, info, outline, fields, images), `raster.rs` (`hayro`), `worker.rs` (the stdin and stdout protocol) |
| `runtime/devagotchi` | the hidden `pdf-worker` verb, the `pdf` and `fonts` verbs, the `pdf` feature |
| `platform/contracts/src/files.rs` | the inputs, answers, operations, limits and request methods |
| `engine/machine-engine/src/files/pdf.rs`, `broker/files.rs`, `broker.rs` | resolve and read inputs, the gate, the child with deadline and memory cap, atomic writes, the `pdf` settings |
| `interfaces/mcp/src/adapter/file_tools.rs` | both tools locally |
| `engine/hub/src/machines/files.rs`, `mcp/`, `site/style.rs`, `site/pwa.rs`, `tests/hub_probe.py` | both tools through a hub; serving `/fonts/`; the service worker caching them |
| `scripts/stage-fonts.sh` (new), `scripts/stage-downloads.sh`, `dist-workspace.toml`, the installers, `runtime/link/src/update/mod.rs`, `deploy/compose/Dockerfile` | the pack: source files, `woff2`, archives, installs, updates, the image |
| `deny.toml`, `LICENSE` notices | BSD-2-Clause; the font licenses |

Each `.rs` file stays at 500 lines or fewer, and the layout ratchet holds.

| Step | What | Accepted when |
|---|---|---|
| P10a | `crates/pdf` create path, the pack loader, the `pdf render` verb, size tuning measured inside `devagotchi` | Golden tests for each row of §8 checked by text extraction; a CJK and an emoji sample render from fonts on disk, and without them give the warning; a runaway document is killed at the deadline; `#`, `$`, `<`, `@`, `\` and `*` stay literal |
| P10b | every `write_pdf` operation | each checked by page count, text and `qpdf --check` and `pdfinfo` with no warnings; AES-256 encrypt and decrypt round-trip; an encrypted input without a password gives `pdf_encrypted` |
| P10c | `read_pdf` | each `what` on the golden PDFs and on PDFs from other producers (LibreOffice, Chrome, a scan); text order compared with `pdf-extract` |
| P10d | the worker child, the broker, `devagotchi mcp`, the gate, limits, settings and logging | probes through `devagotchi mcp` for create, change in place with a SHA, `already_exists`, `changed_since_read`, `outside_workspace` on a granted broker, a killed worker; passwords absent from every log |
| P10e | through the hub, and the e2e | `hub_probe.py` lists both tools; e2e writes and reads a PDF on the test machine through the hub; the parity row turns to present |
| P10f | the font pack shipped | the archives for all four targets hold the pack; the installers place it; a machine update replaces it and checks it; the hub serves it and the phone app shows it offline; the real archive size is recorded here |
| P10g | self-hosted docs and `diag check` | a fresh Debian machine and a fresh Windows machine install from the docs alone, `diag check` reports the pack and the converters correctly |

No CI work until the working alpha, as the user said. Each step lands on main when it works.

## 14. Choices for hades

1. **The app's own text font.** Keep each device's system font for the app's text (the pack
   still gives PDFs, code and previews their fonts), or ship one sans family in the pack so
   the app looks the same on every device. Recommended: ship one, Geist (OFL-1.1), the face
   the website already uses, so the website, the phone app and the desktop match.
2. **CJK and emoji fonts.** Put them in every download (about 17 MB more, so roughly 42 MB
   for Linux x86_64), or fetch them the first time a document or the app needs them.
   Recommended: fetch on first need, checked like an update, so the download stays about
   25 MB and a machine that never meets CJK never pays for it.

Everything else here is a recommendation that hades can change on review.
