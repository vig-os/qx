# Operator cheatsheet

One-page reference for the lab-floor workflow. Print and pin near the
Brother QL-820NWBc.

App: <https://morepet.github.io/part-registry/>
Repo: <https://github.com/MorePET/part-registry>

## What the labels mean

Every part carries a **12-character ID** drawn from the no-lookalike
alphabet `23456789 ABCDEFGHJKMNPQRSTUVWXYZ` — no `0`/`O`, no `1`/`I`/`L`.
The QR and the printed text encode the **same 12 characters**. Either
one alone is enough to look the part up.

Display is always **three rows of four** (4-4-4):

```
K7M3
PQ9R
T5VA
```

Read aloud as three groups. Transcribe in groups of four. The QR is
ID-only (no URL) — labels stay valid even if the lookup app moves.

## Daily workflow (web app)

1. **Open the app** on your phone. Add to Home Screen on first use
   (PWA — one-tap launch, works offline for lookup).
2. **Lookup tab** → tap the camera icon → scan the QR. Or paste/type
   the 12-char ID (case-insensitive, dashes ignored).
3. The detail view shows: status (`unbound` / `bound` / `void`), batch,
   mint date, bind date, type, vendor, part number, location, notes.
4. **To bind** an unbound part: tap **Bind** → fill the form → tap
   **Queue**. Repeat for the rest of the session.
5. **To submit**: open the **Bind queue** → review → tap
   **Submit batch**. This opens **one** PR with all queued binds.

> Note: today the submit step is **stubbed** — the queue persists in
> `localStorage` and the button logs the rows. Real PR creation lands
> with the OAuth work (issue #1). Use the CLI for binds that must merge
> today.

## CLI binding (power-user path)

```bash
uv run bind.py K7M3PQ9RT5VA \
    --type "PT100 1/3 DIN class B, 4-wire" \
    --vendor "TC Direct" --part-number "402-141" \
    --location "sdmd_v2 / cooling-loop / supply-T"
```

8-char prefix accepted (`K7M3PQ9R`). On collision the CLI prints matches
and refuses. `--rebind` overwrites; `--void` retires a spoiled sticker.

## Printing — Brother QL-820NWBc

The QL-820NWBc speaks **AirPrint** over Wi-Fi (Bonjour / mDNS). No
driver install on iOS / iPadOS / macOS. On Linux/Windows add it via
CUPS / system Bluetooth / Wi-Fi.

**DK roll → `--tape` preset → printable height:**

| DK roll    | Tape width | Preset    | Printable | Typical use            |
|------------|-----------:|-----------|----------:|------------------------|
| DK-22214   |      12 mm | `dk-12`   |     10 mm | Small parts, fittings  |
| DK-22210   |      29 mm | `dk-29`   |     25 mm | Standard parts, sensors|
| DK-22225   |      38 mm | `dk-38`   |     33 mm | Modules, sub-assemblies|
| DK-22205   |      62 mm | `dk-62`   |     56 mm | Crates, large housings |

**From the web app (Print tab):**

1. Pick layout (`horz` / `vert` / `flag`) and tape preset.
2. Multi-select IDs (or a whole batch) → **Print**.
3. The OS print dialog opens with **one page per label**. Pick the
   Brother. Confirm. The printer **auto-cuts between pages** on
   continuous DK tape.

**From the CLI:**

```bash
# 1. Render SVGs at the right tape size:
uv run label.py --batch B-2026-05-sdmd --layout horz --tape dk-12

# 2. Convert to single-page PDFs (one cut per file):
cd labels/B-2026-05-sdmd-horz-dk-12/
for f in *.svg; do rsvg-convert -f pdf "$f" -o "${f%.svg}.pdf"; done

# 3. Send to the printer:
lp -d Brother_QL_820NWBc *.pdf
```

`lpstat -p` lists the discovered printer name. Page-per-file = cut-per-file
on continuous tape.

### When the printer jams

1. Power off. Open the lid (release lever at the back).
2. Pull tape **forward** (the direction it normally feeds). Never
   backwards — that wedges the cutter.
3. Trim the leading edge **straight** with scissors before reloading.
   A diagonal edge re-jams.
4. Power on. Hold the **CUT** button until the green light stops
   blinking — this re-aligns the head.
5. Reprint. The driver re-queues failed pages automatically.

If the cutter sticks: hold **CUT** for ~5 s with the lid open to
manually drive the blade through one cycle. If that fails, the cutter
unit is a user-replaceable cartridge — don't pry at it.

## Reporting bugs

Tap the **bug icon** in the top toolbar (Lucide `bug` glyph — replaces
the earlier emoji). It:

1. Captures a screenshot of the current view to your clipboard.
2. Opens a pre-filled GitHub issue in a new tab — title, environment
   (browser / viewport / app version), and a body skeleton.

**Paste the screenshot into the issue body** before submitting.

Include:

- The 12-char ID(s) involved (if any).
- What you did, what you expected, what happened.
- The browser + OS (the plugin fills this in).
- Whether you were online / offline.

Issues land in <https://github.com/MorePET/part-registry/issues>.

---

See [`quickstart.md`](quickstart.md) for the new-operator walkthrough,
or the top-level [`README`](../README.md) for the system overview.
