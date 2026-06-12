# Quickstart for new operators

A narrative walkthrough for your first day on the part-registry. By the
end you'll have the app installed on your phone, scanned a real label,
queued a bind, and printed a label on the Brother QL-820NWBc.

If you just want a one-page reference to pin near the printer, see
[`operator-cheatsheet.md`](operator-cheatsheet.md). Skim this once,
then keep the cheatsheet next to the bench.

## What this system is

Every physical part in the lab — every PT100, fitting, cable, module —
gets a permanent **12-character ID** the first time it enters the
registry. The ID is printed on a label as both a QR code and human-
readable text. From then on the ID is the part's name: it shows up in
calibration logs, incident reports, BoMs, ADRs.

The point of this is **per-instance traceability**. Two PT100s of the
same type are not interchangeable once one of them has been recalibrated
or used in a failed run. The ID lets us tell them apart forever.

The IDs are **mint-then-bind**: a roll of stickers gets pre-printed
with random IDs, the stickers go on parts as parts arrive, and only
afterwards does someone register what each ID is on (its type, vendor,
location). This matters because it means the sticker on a part can be
applied before anyone knows what the part is — which is how parts
actually arrive in a lab.

For the design rationale, see
[ADR-012](../decisions/ADR-012-part-identification.md) (the ID scheme)
and [ADR-013](../decisions/ADR-013-parts-registry-web-app.md) (the web
app).

## Step 1 — Install the PWA on your phone

The web app lives at <https://morepet.github.io/part-registry/>. It's
a Progressive Web App: install it once and it behaves like a native
app — one-tap launch, no browser chrome, works offline for lookups.

**iOS / iPadOS (Safari):**

1. Open the URL in Safari (not Chrome — only Safari can install PWAs
   on iOS).
2. Tap the **Share** icon (square with arrow) in the bottom toolbar.
3. Scroll down → **Add to Home Screen**.
4. Confirm the name (default: "Part Registry") → **Add**.
5. The icon appears on your home screen. Tap to launch.

**Android (Chrome / Edge):**

1. Open the URL.
2. Three-dot menu → **Install app** (or **Add to Home Screen** on
   older Chrome).
3. Confirm. The icon lands on the home screen.

**Desktop (Chrome / Edge / Safari Tech Preview):**

1. Open the URL.
2. The address bar shows an "Install" icon on the right.
3. Click → confirm. It opens as a standalone window thereafter.

> Note: on iOS, the **Add to Home Screen** prompt only appears in
> Safari. Other browsers can browse the site but can't install it.

After installing, launch the app once while online so the service
worker caches the assets. After that, the **Lookup tab works offline**
against the registry snapshot loaded at last online launch.

## Step 2 — Your first scan

Find a part with a label on it. Any part — there should be a few on
the bench already. The label looks like this:

```
+--------+--------+
|        |  K7M3  |
|  [QR]  |  PQ9R  |
|        |  T5VA  |
+--------+--------+
```

Two equal-size square blocks: QR on one side, the same 12 characters
in 4-4-4 on the other. The QR contains **only the 12-char ID** — no
URL, no metadata. (That's deliberate: the label outlives any web app
or domain. See ADR-012 if you're curious.)

In the app:

1. Open the **Lookup** tab. It's the default landing tab.
2. Tap the camera icon. The browser will ask for camera permission
   the first time — say yes. (Permission is per-origin, so you grant
   it once for `morepet.github.io`.)
3. Hold the phone over the QR. The app decodes QR and Micro QR via
   a bundled WebAssembly build of ZXing-C++ (loaded once on the first
   scan), so it works the same on every browser — within a second of
   the QR being in frame.
4. The detail page loads with the part's record.

If the QR is damaged or out of camera range, **type the 12 characters**
into the search field instead. Case doesn't matter; dashes and spaces
are stripped. An 8-char prefix works too — if it's unique, the app
loads that part; if not, you get a list to disambiguate.

The detail view shows everything in `registry.csv` for this part:

| Field         | Meaning                                                  |
|---------------|----------------------------------------------------------|
| `id`          | The canonical 12 chars                                   |
| `status`      | `unbound` (just a sticker), `bound` (on a part), `void`  |
| `minted_at`   | When the ID was generated (ISO-8601 UTC)                 |
| `batch`       | The mint batch — useful for tracing a roll of labels     |
| `bound_at`    | When the bind was recorded                               |
| `type`        | What the part is: "PT100 1/3 DIN class B, 4-wire"        |
| `description` | Free text                                                |
| `vendor`      | Where it came from                                       |
| `part_number` | The vendor's SKU                                         |
| `location`    | Where it lives: "sdmd_v2 / cooling-loop / supply-T"      |
| `notes`       | Free text                                                |

Empty fields are normal for unbound parts.

## Step 3 — Your first bind

Find an **unbound** part — a sticker on a part where nobody's filled
in the metadata yet. Scan it. The detail view will show
`status: unbound` and most fields empty.

1. Tap **Bind** (button on the detail view, or open the Bind tab and
   it pre-fills with the most recent scan).
2. Fill the form:
   - **Type** — most important. Use a consistent format:
     `"PT100 1/3 DIN class B, 4-wire"`, not `"temp sensor"`. If a
     similar part already exists in the registry, copy its `type`
     verbatim so they group cleanly in queries.
   - **Vendor** + **Part number** — what you'd order to replace it.
   - **Location** — slash-separated path:
     `sdmd_v2 / cooling-loop / supply-T`. Keep the same vocabulary
     used elsewhere in the project so location filters work.
   - **Description** / **Notes** — anything else worth remembering.
3. Tap **Queue**. The bind goes into a local queue (persisted in
   `localStorage`, survives a page reload).
4. Repeat for the next part. The queue counter in the toolbar
   increments.

When you've done a session's worth (5, 10, 30 — whatever fits the
shift):

1. Open the **Bind queue** view.
2. Review each pending bind. Edit or remove any that look wrong.
3. Tap **Submit batch**. This is supposed to open one PR against
   `MorePET/part-registry` containing all the queued binds.

> Note: **submit is currently stubbed.** The queue is real and
> persistent, but the submit button logs the rows and shows an alert.
> Real PR creation lands with the OAuth device-flow work (issue #1).
> Until then, for binds that must reach `main` today, use the CLI:
>
> ```bash
> cargo run --bin bind -- K7M3PQ9RT5VA \
>     --type "..." --vendor "..." --part-number "..." \
>     --location "..."
> ```
>
> Then commit and push `registry.csv`. The web queue lets you stage
> the work on the floor; you transcribe it on the laptop afterwards.
> When the OAuth path lands, the queue submission becomes one tap.

## Step 4 — Your first print

The lab's printer is a **Brother QL-820NWBc** loaded with continuous
DK tape. It supports AirPrint over Wi-Fi, so any device on the same
network can print to it through the OS print dialog — no driver
install on macOS, iOS, or iPadOS.

**Pick the right tape preset.** The Brother takes several DK roll
widths; the `--tape dk-N` preset matches the printable height to the
roll:

| DK roll    | Tape | Preset    | Printable | Use                       |
|------------|-----:|-----------|----------:|---------------------------|
| DK-22214   | 12mm | `dk-12`   |     10 mm | Small parts, fittings     |
| DK-22210   | 29mm | `dk-29`   |     25 mm | Standard parts, sensors   |
| DK-22225   | 38mm | `dk-38`   |     33 mm | Modules, sub-assemblies   |
| DK-22205   | 62mm | `dk-62`   |     56 mm | Crates, large housings    |

Check what's loaded in the printer before you print — there's usually
a label visible through the lid. Mismatches don't damage anything but
the labels come out wrong-sized.

**From the web app:**

1. Open the **Print** tab.
2. Filter to the IDs you want: a single ID, a batch, or all `unbound`.
3. Pick the **layout**:
   - `horz` — QR left, text right. Default for flat surfaces.
   - `vert` — QR on top, text below. For narrow strips, cables,
     PCB silkscreen channels.
   - `flag` — `horz` mirrored across a cable wrap. The label wraps
     around a cable so both sides are readable.
4. Pick the **tape** preset to match what's loaded.
5. Tap **Print**. A child window opens with one `@page` per label,
   sized in mm to match the printable height. The OS print dialog
   appears.
6. Pick the Brother (it's discovered automatically over Bonjour) and
   confirm. The printer **auto-cuts between pages** — one cut per
   label.

**From the CLI** (for batch-printing 50+ labels efficiently):

```bash
# Render SVGs:
cargo run --bin label -- --batch B-2026-05-sdmd --layout horz --tape dk-12

# Convert each SVG to a single-page PDF:
cd labels/B-2026-05-sdmd-horz-dk-12/
for f in *.svg; do rsvg-convert -f pdf "$f" -o "${f%.svg}.pdf"; done

# Print everything (one cut per file):
lp -d Brother_QL_820NWBc *.pdf
```

`lpstat -p` lists the discovered printer name on macOS and Linux. If
the Brother isn't listed, add it via System Settings → Printers (macOS)
or `lpadmin` (Linux); it'll show up automatically once it's on the LAN.

### What to do if the print comes out wrong

**Wrong size** — the loaded DK roll doesn't match the `--tape` preset.
Check the lid; reprint with the correct preset.

**QR doesn't scan** — usually printer head needs a clean. Run
`Maintenance → Clean head` from the printer's web UI (its IP shows up
in `lpstat -v`). Reprint the label.

**Cutter jam** — see the cheatsheet's "When the printer jams" section.
Short version: power off, pull tape forward, trim straight, power on,
hold CUT until the light stops blinking.

## Step 5 — Where to find help

- **Cheatsheet**: [`operator-cheatsheet.md`](operator-cheatsheet.md) —
  pin near the printer.
- **Top-level README**: [`../README.md`](../README.md) — system
  overview, CLI reference, ID scheme summary.
- **ADRs**: [`../decisions/`](../decisions/) — design decisions, with
  rationale. ADR-012 explains the IDs; ADR-013 the web app; ADR-014
  the web app's internal architecture.
- **Issues**: <https://github.com/MorePET/part-registry/issues> — file
  bugs via the **bug icon** (Lucide `bug` glyph) in the app toolbar.
  It captures a screenshot to your clipboard and opens a pre-filled
  issue; paste the screenshot into the body before submitting.
- **Examples**: [`../examples/`](../examples/) — reference label
  renderings at common sizes. Useful for picking a layout before you
  print.

## Porting to another registry (data repo setup)

The web app is designed to work with any data repository that serves a
`registry.csv` file via GitHub Pages. To point the app at your own
registry, set three environment variables at build time:

```bash
# The GitHub org/repo that hosts registry.csv on its gh-pages branch.
VITE_DATA_REPO=your-org/your-registry

# The base path the app is served from (must match your GitHub Pages
# config or your reverse proxy). Include the trailing slash.
VITE_BASE=/your-registry/

# The branch that GitHub Pages serves from (usually main or gh-pages).
VITE_DATA_BRANCH=main
```

For example, to build and deploy for a custom registry:

```bash
VITE_DATA_REPO=acme-lab/acme-parts \
VITE_BASE=/acme-parts/ \
VITE_DATA_BRANCH=main \
npm run build
```

The resulting `dist/` folder can be deployed to any static host. The
app will fetch `registry.csv` from
`https://raw.githubusercontent.com/${VITE_DATA_REPO}/${VITE_DATA_BRANCH}/registry.csv`
at runtime.

## What's next

The on-site help is being built incrementally. Tracked in
[issue #8](https://github.com/MorePET/part-registry/issues/8):

- Inline help icons on each tab that open relevant docs without
  leaving the app.
- A dismissable Quickstart panel on the Lookup tab for first-time
  users.
- A printable cheatsheet route inside the app (currently the markdown
  in this folder is the source).
- Screenshots in the cheatsheet once the UI stabilizes.

If anything in this document is wrong, unclear, or missing — file an
issue. The docs are part of the registry, and they get the same
treatment: every change is a PR, every fix is a commit.
