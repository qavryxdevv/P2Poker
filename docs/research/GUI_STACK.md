# GUI stack research — Phase 0

Scope: spec `SPEC_CS.md` §22 (GUI and look), §23 (source layout, `gui/{lobby,table,theme}.rs`),
§28 (dependency security), §33 (crypto must not block the GUI event loop).

Every API claim below carries a **Verification** line. Two kinds of evidence are used, and
nothing else:

* **(a) compiled** — code using the API passed `cargo check` / `cargo build --release` on this
  machine (rustc 1.95.0, cargo 1.95.0, `x86_64-pc-windows-msvc`, Windows 10, 24 cores).
* **(b) source** — the claim was read in the unpacked crate under
  `~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/<crate>-<version>/`.

Probe crates (never inside the repo):

```
<scratchpad>
  <session-id>/scratchpad/
    probe-gui-egui/        eframe 0.36.1 + glow  (full probe: 2 windows, painter, images, worker)
    probe-gui-egui-wgpu/   same source, eframe wgpu renderer (size comparison only)
    probe-gui-slint/       slint 1.17.1 + femtovg (2 windows, markup table, worker)
```

---

## 0. Recommendation up front

**Use `eframe` 0.36.1 with the `glow` (OpenGL) renderer, `default-features = false`, plus
`egui_extras` 0.36.1 (`image` feature) and `rust-embed` 8.12.0 for card art.**

Slint is a competent second choice and was fully built and run, but it loses on three
spec-anchored points, not on taste: it cannot rotate a `Rectangle` (§22 hero cards), it is
GPL-3.0-only unless a separate Slint licence is accepted (§28 licence recording), and it drags
in ~2.5× the dependency graph (§28 "minimum number of dependencies"). Full argument in §7.

---

## 1. Versions, build times, executable sizes

Crate versions confirmed against the crates.io API on 2026-08-28
(`curl -H "User-Agent: p2p-poker-research (<your-email>)" https://crates.io/api/v1/crates/<c>`):

| crate | max_stable_version | updated_at |
|---|---|---|
| `egui` | 0.36.1 | 2026-08-07 |
| `eframe` | 0.36.1 | 2026-08-07 |
| `egui_extras` | 0.36.1 | 2026-08-07 |
| `slint` | 1.17.1 | 2026-07-07 |
| `slint-build` | 1.17.1 | 2026-07-07 |
| `rust-embed` | 8.12.0 | 2026-07-08 |
| `image` | 0.25.10 | 2026-03-10 |

Both probes were built with the same release profile:

```toml
[profile.release]
opt-level = "z"
lto = true
codegen-units = 1
panic = "abort"
strip = true
```

### Measured results

| probe | renderer | CRT | release .exe (bytes) | MiB | build time |
|---|---|---|---|---|---|
| eframe 0.36.1 | glow (GL) | dynamic | 5 470 208 | 5.22 | 1 m 36 s |
| **eframe 0.36.1** | **glow (GL)** | **static** | **5 646 848** | **5.39** | 2 m 42 s (clean, explicit target) |
| eframe 0.36.1 | wgpu | static | 8 380 416 | 7.99 | 2 m 35 s |
| slint 1.17.1 | femtovg (GL) | dynamic | 7 308 800 | 6.97 | ≈ 2 m 50 s |
| **slint 1.17.1** | **femtovg (GL)** | **static** | **7 461 888** | **7.12** | 1 m 36 s (warm registry) |

Static CRT = `RUSTFLAGS="-C target-feature=+crt-static"` with an explicit
`--target x86_64-pc-windows-msvc`. Build times are wall-clock on 24 cores with a warm crates.io
registry; they are indicative, not benchmarks (the slint dynamic-CRT number is a lower bound —
the first run was cut off at 2 m and needed another 50 s).

**Verification (a):** all five binaries were produced by `cargo build --release`; sizes read with
`stat`. The egui `glow` static figure is the final probe, which additionally contains the
`rust-embed` / `image` / font / rotated-mesh code paths; those helpers are `#[allow(dead_code)]`
and LTO removed them, so the number is for the live probe, not for dead code.

**Renderer decision:** `glow` over `wgpu` saves 2.73 MB (33 %) and 24 crates. eframe's *default*
feature set selects `wgpu`; we must therefore set `default-features = false`.

### DLL dependencies — "one executable, no runtime to install" (§22)

PE import tables were dumped directly from the .exe headers (own parser, both the import
directory and the delay-import directory).

Dynamic-CRT builds import **`VCRUNTIME140.dll`** and the `api-ms-win-crt-*` UCRT stubs. That is
the Microsoft redistributable and is **not** guaranteed on a clean Windows install — it breaks
§22. With `+crt-static` both disappear:

* `probe-gui-egui.exe` (static): `ADVAPI32`, `GDI32`, `KERNEL32`, `OPENGL32`, `USER32`,
  `api-ms-win-core-synch-l1-2-0`, `bcryptprimitives`, `dwmapi`, `imm32`, `ntdll`, `ole32`,
  `shell32`, `uxtheme`.
* `probe-gui-slint.exe` (static): the same set plus `combase`, `comctl32`, `dwrite`, `oleaut32`,
  `shlwapi`.

Every remaining name ships with Windows. No delay-loaded imports in either binary.

**Verification (a) + PE header inspection:** both static .exes were copied **alone** into a fresh
empty directory (no `target/`, no DLLs, no assets) and started from there. Both ran. Enumerating
the top-level visible windows per process ID via `EnumWindows` + `GetWindowThreadProcessId`
returned **two windows for each process**.

**Portability side-effect check:** after each run, the empty directory still contained exactly the
two `.exe` files — neither app wrote anything into its own folder. (This is the default; see the
persistence warning in §6.)

**Conclusion for §22:** requirement met by both, *provided* we build with `+crt-static`. Record
this in the build documentation — it is not the cargo default.

---

## 2. Multi-window (poker table in a separate window)

### egui / eframe — yes, verified

`eframe` 0.36 changed the `App` trait. It is **not** `update(&mut self, &Context, &mut Frame)` any
more:

```rust
// eframe-0.36.1/src/epi.rs:152
pub trait App {
    fn logic(&mut self, ctx: &egui::Context, frame: &mut Frame) { _ = (ctx, frame); }
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut Frame);   // line 182
    fn save(&mut self, _storage: &mut dyn Storage) {}
    // ...
}
pub type AppCreator<'app> =                                   // line 49
    Box<dyn 'app + FnOnce(&CreationContext<'_>) -> Result<Box<dyn 'app + App>, DynError>>;
```

**Verification (b):** `eframe-0.36.1/src/epi.rs` lines 49–50, 152–182.
**Verification (a):** the probe implements exactly this and compiles.

A second **native OS window** is a deferred viewport. `Context::show_viewport_deferred` takes a
callback of `impl Fn(&mut Ui, ViewportClass) + Send + Sync + 'static` — note it hands you a `Ui`,
not a `Context`:

```rust
// egui-0.36.1/src/context.rs:4059
pub fn show_viewport_deferred(
    &self,
    new_viewport_id: ViewportId,
    viewport_builder: ViewportBuilder,
    viewport_ui_cb: impl Fn(&mut Ui, ViewportClass) + Send + Sync + 'static,
)
```

Working probe code (compiles, and the window really opens):

```rust
if self.table_open.load(Ordering::Relaxed) {
    let flag = self.table_open.clone();
    ctx.show_viewport_deferred(
        ViewportId::from_hash_of("poker-table"),
        ViewportBuilder::default()
            .with_title("Poker table")
            .with_inner_size([1000.0, 700.0]),
        move |ui: &mut egui::Ui, class: ViewportClass| {
            debug_assert!(class == ViewportClass::Deferred);
            let rect = ui.max_rect();
            paint_table(ui.painter(), rect);
            if ui.ctx().input(|i| i.viewport().close_requested()) {
                flag.store(false, Ordering::Relaxed);   // user closed the table window
            }
        },
    );
}
```

`ViewportClass` is `Root | Deferred | Immediate | <fallback>` and does **not** implement `Debug`
(`debug_assert_eq!` on it fails to compile; use `debug_assert!(a == b)`).

**Verification (b):** `egui-0.36.1/src/context.rs:4059`, `egui-0.36.1/src/viewport.rs:82`.
**Verification (a):** compiles, and the running process shows two top-level windows.

*Deferred* is the right class for us: the spec's table window repaints on a poker clock while the
lobby idles, and `show_viewport_immediate` is documented in-source to force both viewports to
repaint together ("double work for two viewports").

### Slint — yes, verified

Each `export component X inherits Window` becomes an independent native window with its own
generated Rust type and `show()`:

```rust
slint::include_modules!();
let main  = MainWindow::new()?;
let table = TableWindow::new()?;
table.show()?;
main.run()
```

**Verification (a):** compiles and runs; the process shows two top-level windows.

Both frameworks satisfy §22's "poker table in a separate window". No differentiator here.

---

## 3. Custom drawing: felt, seats, cards

### egui `Painter` — everything the reference image needs exists

Confirmed in `egui-0.36.1/src/painter.rs` and `epaint-0.36.1/src/shapes/shape.rs`, and exercised
in the probe:

| need | API | evidence |
|---|---|---|
| table oval | `Shape::Ellipse(EllipseShape { center, radius: Vec2, fill, stroke, angle })` | (b) `epaint/src/shapes/ellipse_shape.rs:6`; (a) compiles |
| rounded plates | `Painter::rect(rect, impl Into<CornerRadius>, fill, stroke, StrokeKind)` | (b) `painter.rs:380`; (a) |
| gradients | `Mesh` with per-vertex `Color32` (`Mesh::colored_vertex`, `add_rect_with_uv`) | (b) `epaint/src/mesh.rs:169,199`; (a) |
| card corner radius | `Painter::rect_filled(rect, CornerRadius::same(6), fill)` | (b) `painter.rs:397`; (a) |
| suit pips (straight) | `Shape::Path(PathShape::convex_polygon(pts, fill, stroke))` | (a) |
| suit pips (curved) | `Shape::CubicBezier(CubicBezierShape)`, `Shape::QuadraticBezier` | (b) `shape.rs:63–67` |
| tilted hero cards | `Mesh::rotate(Rot2, Pos2)`, `Mesh::transform(TSTransform)`, `Shape::transform` | (b) `mesh.rs:309–325`, `shape.rs:427–443`; (a) |
| tilted card images | `egui::Image::rotate(angle, origin)` | (b) `widgets/image.rs:238`; (a) |
| rotated text | `TextShape { angle, .. }` | (b) `epaint/src/shapes/text_shape.rs:41` |
| text plates | `Painter::text(pos, Align2, text, FontId, Color32)` | (b) `painter.rs:469`; (a) |

Note on **naming churn**: `Rounding` is now `CornerRadius`, and `rect_stroke` requires an explicit
`StrokeKind`. Do not copy 0.2x-era egui examples.

### Slint markup — good, with one hard blocker

Verified compiling in `probe-gui-slint/ui/app.slint`:

* `Rectangle { border-radius: self.height / 2; }` for the felt/rail ovals,
* `@radial-gradient(circle, #2a9c5b 0%, #14663a 100%)` and `@linear-gradient(180deg, …)`,
* `Path { commands: "M 15 0 L 30 26 L 0 26 Z"; fill: white; }` for pips,
* nested `Rectangle` + `Text` for the name/stack plates,
* `Image { source: @image-url("../card_back.png"); }`.

**Blocker — verified negative result.** Slint 1.17.1 can only rotate `Image` and `Text`. Every
attempt to tilt a rectangle-based card failed to compile:

```
error: Unknown element 'Rotate'
error: Unknown element 'Transform'. (The type exists as an internal type, but cannot be accessed in this scope)
error: Unknown property rotation-angle in Rectangle
```

`Image` and `Text` do accept it, but only under the new name (`rotation-angle` compiles with a
deprecation warning: *"The property 'rotation-angle' has been deprecated. Please use
'transform-rotation' instead"*).

**Verification (a):** the three errors above are literal `cargo check` output from the
`slint-build` build script.
**Verification (b):** `i-slint-compiler-1.17.1/builtins.slint` — `Transform` is declared at
line 500 and carries the `//-is_internal` marker; `transform-rotation` aliases exist only on
`ImageItem` (line 375) and `SimpleText` (line 561); `Rectangle` (line 36) has none.

The reference image's hero cards are tilted (≈ ±6°). In Slint they would have to be pre-rendered
to bitmaps and rotated as `Image`s — which forfeits crisp vector text on the card that is closest
to the player. In egui a tilted card is `Mesh::add_rect_with_uv` + `Mesh::rotate` and stays
resolution-independent.

**Verdict:** egui's `Painter` is more workable for this specific design. Slint's markup is nicer
for the *lobby* (declarative lists, `std-widgets` tables), but the table window is the harder half
and egui wins it outright.

---

## 4. Card assets: draw procedurally, ship one image

### Decision

**Draw all 52 faces procedurally. Ship exactly one bitmap: the card back.** Optionally a small
sprite sheet for the four suit pips.

Reasons, grounded in what the reference image actually is (see §5): a face is a flat coloured
rounded rectangle, a small rank glyph and pip in the top-left corner, and one large rank glyph
below. That is four draw calls. Shipping 52 pre-rendered PNGs at a hi-DPI size (~240×336 for a
4× table) would add roughly 1–2 MB to a 5.4 MB executable and would still blur when the table
window is resized, because the table has no fixed size. The *back* is the only element with
non-trivial artwork (an embossed repeating motif), so that one is a bitmap.

### Verified negative result — no suit glyphs in egui's bundled fonts

The `default_fonts` feature bundles `Ubuntu-Light.ttf`, `Hack-Regular.ttf`,
`NotoEmoji-Regular.ttf` and `emoji-icon-font.ttf` (1.4 MB total). Parsing the `cmap` (format 4,
platform 3/1) of the two *text* fonts shows that **none** of `U+2660 ♠`, `U+2661`, `U+2662`,
`U+2663 ♣`, `U+2665 ♥`, `U+2666 ♦` is mapped.

**Verification (b):** direct `cmap` table parse of
`epaint_default_fonts-0.36.1/fonts/Ubuntu-Light.ttf` and `Hack-Regular.ttf` — all six code points
absent.

So `painter.text(.., "♠", ..)` will render tofu. Pips must come from our own asset: either four
white glyphs in a tiny embedded sprite PNG (tinted per card), or `CubicBezierShape` paths for
♥/♣ and `PathShape::convex_polygon` for ♦/♠. The sprite is simpler and one file.

### Embedding — both paths compile

**Path A, egui-native (recommended for a handful of files):**

```rust
egui_extras::install_image_loaders(&cc.egui_ctx);          // once, in App::new
let src: egui::ImageSource<'static> = egui::include_image!("../assets/cards/back.png");
ui.add(egui::Image::new(src).fit_to_exact_size(Vec2::splat(48.0)));
```

`include_image!` bakes the bytes into the binary; `egui_extras`' image loader decodes them at
runtime. Requires `egui_extras = { version = "0.36.1", features = ["image"] }` and the matching
`image` feature for the format (`image = { version = "0.25.10", features = ["png"] }`).

**Path B, rust-embed (recommended once `assets/cards/` has many files — §23 expects that folder):**

```rust
#[derive(rust_embed::Embed)]
#[folder = "assets/"]
struct Assets;

fn decode_embedded_card(ctx: &egui::Context) -> Option<egui::TextureHandle> {
    let f = Assets::get("cards/back.png")?;
    let img = image::load_from_memory(&f.data).ok()?.to_rgba8();
    let (w, h) = img.dimensions();
    let ci = egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], img.as_raw());
    Some(ctx.load_texture("card_back", ci, egui::TextureOptions::LINEAR))
}
```

Note the derive macro is `rust_embed::Embed` (the old `RustEmbed` name is legacy).

**Font, embedded, no OS font needed** (needed for the card rank glyphs and for a stable look
across machines):

```rust
fn install_card_font(ctx: &egui::Context, ttf: &'static [u8]) {
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert("cards".into(), std::sync::Arc::new(egui::FontData::from_static(ttf)));
    fonts.families.entry(egui::FontFamily::Name("cards".into())).or_default().push("cards".into());
    ctx.set_fonts(fonts);
}
```

**Verification (a):** all three snippets are in `probe-gui-egui/src/main.rs` and pass
`cargo check`; the `include_image!` one also runs.

**Size lever:** if we ship our own font we can drop `default_fonts` and save most of that 1.4 MB,
taking the executable to roughly 4.3–4.5 MB.

**Slint equivalent, for the record:** `@image-url("…")` is embedded at build time by default. The
generated code contains
`static SLINT_EMBEDDED_RESOURCE_0 : &'static [u8] = ::core::include_bytes!("…/card_back.png");`
**Verification (b):** the generated `…/build/probe-gui-slint-*/out/app.rs:2944`. The behaviour is
controlled by `slint_build::CompilerConfiguration::embed_resources(EmbedResourcesKind::EmbedFiles)`
(**(b)** `slint-build-1.17.1/lib.rs:76–90,163`).

---

## 5. Visual brief for `gui/theme.rs` — reading of `assets/ggpoker-rush-and-cash-table.jpg`

Source image is **800 × 548 px**. All pixel figures below were measured on that image (edge scans
for geometry, median-cut dominant-colour extraction over hand-picked clean rectangles for colour).
Colours are *sampled from a JPEG*, so treat them as accurate to a few units per channel; the ones
that sit under text anti-aliasing (the cyan stack figures) are stated as the saturated peak, which
is the honest reading of the intended colour.

### 5.1 Overall composition

A single wide oval table fills most of the frame, floating on a dark warm-brown background. Six
seats sit **on** the table's rail: the hero at bottom-centre, one at top-centre, and two on each
side at the far left and far right of the oval — not evenly spaced by angle, but pushed out to the
oval's extremes. The board is a horizontal row of five cards across the middle of the felt, the
pot **label** sits just above the board, and the pot **chips** sit just below it. Action controls
occupy the bottom-right corner outside the table.

### 5.2 Geometry (image px, and as fractions of the window)

| element | measurement | fraction |
|---|---|---|
| felt (inner) ellipse | centre (399, 263), semi-axes a = 317, b = 133 | cx 0.499 W, cy 0.480 H, a 0.396 W, b 0.243 H |
| rail (outer) ellipse | centre ≈ (399, 277), a ≈ 344, b ≈ 161 | a 0.430 W, b 0.294 H |
| rail thickness | 26 px at the sides, ~13 px at the top, ~42 px at the bottom | the rail ellipse is offset ~14 px **down** from the felt ellipse |
| table bounding box | x 55…742, y 117…438 | 0.859 W × 0.586 H |
| felt aspect | a : b = 2.38 : 1 | a wide, shallow oval — not a circle |

Do **not** draw one concentric ring. The outer ellipse is pushed down relative to the inner one,
which is what makes the front rail look thick and the back rail look thin — a cheap, effective
fake perspective. Reproduce that offset.

### 5.3 Colours

**Background (outside the table)** — a soft vertical vignette in warm brown-grey, brightest at the
middle of the frame:

```
top    #1C1311
middle #302423
bottom #1E1614
```

**Rail (wood).** Not a flat colour: dark rosewood at the top and sides, lit sand at the
bottom-front, as if lit from below.

```
top rail          #6B4A3E  (darkening to #3E2C25 at the very top edge)
left / right rail #8B5A4C   (sampled #8A594B left, #84594B right)
bottom rail       #CBAF94 … #D2BA9E   (the lit band)
outer edge line   #5B4035
inner edge line   #6B452B  (a thin dark keyline where wood meets felt)
```

The bottom rail carries the brand engraved in wide letter-spaced capitals — in the original,
`R U S H   &   C A S H`, drawn slightly darker than the rail (≈ #8B6A55 on #CBAF94), following the
ellipse. A faint mirrored version of the same lettering runs along the top rail. For us this
becomes `P 2 P   P O K E R`.

**Felt.** A large soft radial highlight centred slightly below the geometric centre, dark green at
the edges, brighter green in the middle where the cards sit.

```
felt edge     #00411E
felt mid ring #004A24
felt centre   #006C3A   (peak highlight, around (400, 325))
felt keyline  #0D4A2A
```

Vertical profile measured at x ≈ 200: `#013B1A` at y 150 → `#004A24` at y 270 → `#005027` at
y 330 → `#005027` at y 390. Horizontal profile at y ≈ 328: `#004D25` at x 160 → `#006C3A` at
x 340–400 → `#00592D` at x 520. Model it as a radial gradient with the focus at roughly
(0.50 W, 0.60 of the felt's vertical extent).

### 5.4 Cards

**A four-colour deck where the entire card face is the suit colour.** This is the single most
distinctive thing in the image and it must be reproduced.

| suit | face colour (sampled) | suggested gradient |
|---|---|---|
| ♦ diamonds | `#043E7B` (dominant 66 %) | `#0A50A0` → `#043E7B`, top to bottom |
| ♣ clubs | `#036905` | `#0A8210` → `#036905` |
| ♠ spades | `#131313` | `#2A2A2A` → `#101010` |
| ♥ hearts | `#960607` | `#AD0A0B` → `#8B0607` |

Rank and pip are **white** (`#FFFFFF`, sampled peak `#EFFEF7`).

Board card metrics, measured by scanning the row y = 250 for non-felt runs:

* card size **60 × 84 px** (aspect 1 : 1.40), corner radius ≈ 6 px, **no border** on board cards
* pitch 68 px → 8 px gap between cards
* the five cards span x 233…563, y 209…292 → centred at (398, 250) on an 800-px window
* the row is 330 px wide = 52 % of the felt width; one card is 9.5 % of the felt width and 32 % of
  the felt height

Layout inside a face: a small rank glyph at the top-left (cap height ≈ 18 px on a 60 px card,
so ~30 % of card width), the suit pip directly under it (≈ 14 px), and one **large** rank glyph
filling the lower two-thirds (cap height ≈ 44 px, over half the card height), left-ish and
bottom-anchored so tall glyphs get clipped by the card's lower edge. The typeface is a heavy
condensed serif/slab — think a bold Roman with visible stem contrast, not a geometric sans.

**Card back** (`assets/cards/back.png`, the one bitmap we ship): dusty brick red `#912F31` with a
subtle darker embossed repeating motif, a thick **white border ≈ 4–5 px** and a corner radius
≈ 8 px. Opponent hole cards are drawn as two of these, overlapping by roughly 40 % and each
tilted a few degrees.

**Hero cards:** larger than board cards (≈ 70 × 100 px at this scale), each with the same thick
white border, **tilted ≈ −6° and +6°**, overlapping, rising out from behind the hero's name plate.
Their big rank glyphs are clipped by the plate. (This tilt is the requirement Slint cannot meet —
see §3.)

### 5.5 Seat plates

Structure, from top to bottom, all horizontally centred on the seat:

1. **Avatar** — a circle ≈ 66 px diameter with a light grey ring ≈ 3 px (`#B9BCC0`, sampled peak
   `#FCF8F3`) and a dark inner ring, overlapping the top edge of the plate.
2. **Plate** — a rounded rectangle ≈ 118 × 46 px, corner radius ≈ 8 px, in two stacked rows:
   * **name row** — charcoal `#1B1B1E`, name centred in near-white `#F2EFE8` at ≈ 13 px. The
     hero's own name reads a shade warmer than the opponents' plain white.
   * **stack row** — near-black `#101215`, amount centred in bright cyan. Measured saturated peak
     on the hero's own (largest, least JPEG-smeared) figure: **`#29BBD9`**; use ≈ `#45BFE3` at
     ≈ 15 px semi-bold. Format is `44.5 BB` — big blinds, not chips.
3. **Counter badge** — a small square overhanging the plate's **top-left** corner, ≈ 26 × 20 px,
   black fill with a thin dark-red keyline and a white number.
4. **Country flag** — ≈ 30 × 20 px overhanging the plate's **top-right** corner, thin light border.
5. **Action timer** — for the player to act only, a thin bar directly under the plate running a
   green → yellow gradient as it depletes.
6. **Hand description** — under the hero plate only, a black pill with white text
   (`two pair, Ks and 8s`).

Seat centres, measured, given as fractions of the window so the layout scales:

| seat | image px | fraction (W, H) |
|---|---|---|
| hero (bottom-centre) | (400, 503) | (0.50, 0.92) |
| lower-left | (112, 378) | (0.14, 0.69) |
| upper-left | (112, 168) | (0.14, 0.31) |
| top-centre | (400, 113) | (0.50, 0.21) |
| upper-right | (688, 168) | (0.86, 0.31) |
| lower-right | (688, 378) | (0.86, 0.69) |

That is a **6-max** layout. For heads-up (§32, the first supported mode) use hero at (0.50, 0.92)
and the opponent at (0.50, 0.21).

### 5.6 Pot, chips, dealer button

* **Pot label pill** — 95 × 18 px at (352…447, 185…203), i.e. centred (399, 194), fully rounded
  (radius = h/2 = 9), sitting **24 px above the top edge of the board**. Fill very dark green
  `#0A2410` (sampled `#0C1F06`), a slightly lighter keyline, text in pale gold — measured
  `#EEE08A`, peak `#FFEFA3` — reading `Total Pot : 11.5 BB` at ≈ 11 px.
* **Pot chips** — a small stack of 2–3 chips just **below** the board centre at ≈ (400, 297), with
  the amount `10.5 BB` in white directly under it at y ≈ 310.
* **Committed bets** — a single chip on the felt in front of the seat that bet, ≈ 14 px diameter
  with a light rim, and a small white amount label beside it (`1 BB` at (635, 338) in the sample).
* **Dealer button** — a gold disc ≈ 18 px, fill `#F1D727` (peak `#FFF16D`), black `D`, resting on
  the felt just inside the rail next to the button seat (at (176, 365) in the sample).

Note the spec phrase "board above the pot" resolves cleanly here: the pot *chips* are at the
centre below the board, and the pot *label* is above the board.

### 5.7 Action panel (bottom-right)

* **Quick-bet row** — four small rounded rectangles `33% / 50% / 75% / 100%`, ≈ 44 × 26 px, fill
  `#3D4041` with a 1 px lighter keyline `#6A6E70`, white bold text ≈ 13 px. These are §22's
  "quick bets", relabelled as ½ pot / pot / all-in for us.
* **Bet amount field** — a black rectangle ≈ 70 × 26 px with the number right-aligned in
  grey-white.
* **Bet slider** — a horizontal track with a gold-rimmed poker-chip knob ≈ 26 px as the handle.
* **Action buttons** — three rounded rectangles ≈ 95 × 46 px, radius ≈ 6, in red with a vertical
  gradient and a subtle bevel (lighter top edge, dark bottom edge): `#D45751` top → `#A02C2A`
  bottom, dominant mid `#BC3835`. Labels in bold white, two lines where needed
  (`Call` / `1 BB`, `Raise to` / `2 BB`). Label highlight sampled `#FDDDDA`.

### 5.8 What we replace

The original's top-left "BAD BEAT / 2,222,365.20" jackpot plaque, the casino promo icons, the
emoji/chat/video icon row and the "Global Cash Game Sit-out" checkbox are GG product furniture.
The plaque's slot in the top-left is a good home for §22's required **protocol/security status**
(DHT / libp2p / lobby state, shuffle-proof verification result), and the icon row for the chat
control.

**Verification for all of §5:** direct pixel measurement of
`assets/ggpoker-rush-and-cash-table.jpg` — edge runs
for geometry, median-cut dominant colours over clean rectangles, saturation-ranked sampling for
anti-aliased text colours. Colour values are estimates from a lossy JPEG and should be treated as
a starting palette, not as ground truth.

---

## 6. Keeping crypto off the UI thread (§33)

The Bayer–Groth style shuffle proofs the crypto track is evaluating take hundreds of milliseconds
to seconds per hand. Neither framework may block on them.

### egui — the idiomatic pattern

`egui::Context` is cheap to clone and is `Send + Sync + 'static`, so the worker can hold one and
call `request_repaint()` to wake the UI. That is the whole mechanism: **channel in, channel out,
`request_repaint()` to wake, `try_recv()` in `ui()`.**

```rust
enum ToWorker   { Shuffle(u64) }
enum FromWorker { ShuffleDone { hand: u64, proof_len: usize } }

struct Worker { tx: Sender<ToWorker>, rx: Receiver<FromWorker> }

impl Worker {
    fn spawn(ctx: egui::Context) -> Self {
        let (tx_in, rx_in)   = channel::<ToWorker>();
        let (tx_out, rx_out) = channel::<FromWorker>();
        std::thread::Builder::new()
            .name("mental-poker".into())
            .spawn(move || {
                while let Ok(job) = rx_in.recv() {
                    match job {
                        ToWorker::Shuffle(hand) => {
                            let proof_len = expensive_bayer_groth(hand);   // seconds of CPU
                            let _ = tx_out.send(FromWorker::ShuffleDone { hand, proof_len });
                            ctx.request_repaint();      // wake the UI thread
                        }
                    }
                }
            })
            .expect("spawn worker");
        Self { tx: tx_in, rx: rx_out }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        while let Ok(msg) = self.worker.rx.try_recv() {      // never blocks
            match msg { FromWorker::ShuffleDone { hand, proof_len } => { /* update state */ } }
        }
        // …draw…
    }
}
```

`std::thread::Builder::spawn` requires `Send + 'static` on the closure, and the closure captures
`egui::Context` — so the fact that this **compiles** is the proof that `Context` is `Send`.
**Verification (a):** exactly this code is in `probe-gui-egui/src/main.rs` and passes
`cargo check` and `cargo build --release`.

For a network-driven client we will want tokio anyway (libp2p); the same shape works with
`tokio::sync::mpsc` + `spawn_blocking` for the CPU-bound proof work, keeping the async runtime's
worker threads free.

### Slint — equivalent, verified

Slint's UI handles are `!Send`, so you keep a `Weak<T>` and hop back with
`slint::invoke_from_event_loop`:

```rust
let weak = main.as_weak();
std::thread::Builder::new().name("mental-poker".into()).spawn(move || {
    while let Ok(job) = rx.recv() {
        let proof_len = expensive_bayer_groth(job);
        let weak = weak.clone();
        let _ = slint::invoke_from_event_loop(move || {
            if let Some(w) = weak.upgrade() { w.set_status(format!("proof {proof_len} B").into()); }
        });
    }
}).expect("spawn worker");
```

**Verification (a):** in `probe-gui-slint/src/main.rs`, passes `cargo check` and
`cargo build --release`.

Both are fine. egui's is marginally simpler because state lives in one struct the UI owns; Slint's
requires the extra `Weak` upgrade hop.

---

## 7. Recommendation and justification against the spec

**`eframe` 0.36.1 + `glow`, `default-features = false`.**

Suggested manifest, exactly as built and verified:

```toml
[dependencies]
eframe      = { version = "0.36.1", default-features = false, features = ["glow", "default_fonts"] }
egui        = "0.36.1"
egui_extras = { version = "0.36.1", features = ["image"] }
image       = { version = "0.25.10", default-features = false, features = ["png"] }
rust-embed  = "8.12.0"

[profile.release]
opt-level = "z"
lto = true
codegen-units = 1
panic = "abort"
strip = true
```

Build with `RUSTFLAGS="-C target-feature=+crt-static" cargo build --release --target x86_64-pc-windows-msvc`.
Add `#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]` so the release build has
no console window. (The probe includes `wayland`/`x11` features for Linux; they are inert on
Windows and cost nothing there.)

Justification, clause by clause:

1. **§22 "one runnable executable, no runtime to install"** — 5.39 MB single .exe, verified to run
   alone from an empty directory, importing only DLLs that ship with Windows. Slint also passes
   (7.12 MB) but is 32 % larger. `wgpu` would be 7.99 MB, so `glow` is the right renderer.
   Both frameworks need `+crt-static`; that is a build-flag decision, not a framework one.
2. **§22 portable, nothing written outside its own folder** — verified: neither app wrote anything
   during a 6-second run in an empty directory. **Caveat for egui:** eframe's optional
   `persistence` feature (**not** in `default`, **(b)** `eframe-0.36.1/Cargo.toml`) writes to
   `%APPDATA%\<app_id>\data` on Windows (**(b)** `native/file_storage.rs:37`). Either leave the
   feature off and write the profile ourselves via `std::env::current_exe()`, or enable it and set
   `NativeOptions::persistence_path: Option<PathBuf>` (**(b)** `epi.rs:383`) to a path beside the
   executable. Written up so it cannot be forgotten.
3. **§22 separate table window** — `show_viewport_deferred`, verified compiling and verified as a
   real second top-level OS window. Deferred viewports also repaint independently, which matters
   because the table runs an action clock while the lobby is idle.
4. **§22 look modelled on the reference image** — the design in §5 is flat fills, ellipses,
   rounded rectangles, two gradients and text. egui's `Painter` does all of it, and — decisively —
   it can rotate the hero cards, which Slint 1.17.1 cannot do for a rectangle-based card at all
   (three compiler errors quoted in §3). That is the one requirement where the two frameworks
   genuinely differ.
5. **§22 PokerTH-style lobby** — a sortable multi-column table, a player list, a chat pane and
   buttons. Slint's `std-widgets` gives more of this for free; egui needs `egui_extras::TableBuilder`
   (already a dependency for the image loader — **(b)** `egui_extras-0.36.1/src/table.rs:247`,
   re-exported ungated from `lib.rs:30`). This is the one point where Slint is ahead, and it is a
   smaller amount of work than the table window.
6. **§28 minimum dependencies** — the egui probe's dependency graph is **127 crates**; the Slint
   probe's is **311**. Part of Slint's is compile-time only (its `.slint` compiler pulls `image`,
   which pulls `ravif` → `rav1e`, an AV1 *encoder*), but its *runtime* renderer stack alone is
   `femtovg` + `fontdb` + `rustybuzz` + `resvg`/`usvg`/`tiny-skia` + `image` + `image-webp`, all of
   which egui replaces with `glow` + `epaint`. `cargo-deny` and `cargo-audit` (§28) audit build
   dependencies too, so the compile-time half is not free either.
   **Verification (a):** `cargo tree --target x86_64-pc-windows-msvc --edges normal,build`,
   deduplicated by name+version.
7. **§28 licence recording** — egui/eframe/egui_extras are `MIT OR Apache-2.0`. Slint is
   `GPL-3.0-only OR LicenseRef-Slint-Royalty-free-2.0 OR LicenseRef-Slint-Software-3.0`
   (**(b)** `slint-1.17.1/Cargo.toml`). Choosing Slint forces the client to GPL-3.0-only unless we
   affirmatively accept Slint's own royalty-free terms. That is a real project-level constraint
   for an open-source client, not a preference.
8. **§33 no blocking the event loop** — verified compiling worker patterns for both; egui's is the
   simpler of the two (§6).
9. **§23 source layout** — `gui/{lobby,table,theme}.rs` maps naturally onto egui: `theme.rs` is a
   plain palette + geometry module of `Color32` and ratio constants (directly from §5), `table.rs`
   is one `paint_table(&Painter, Rect)` plus hit-testing, `lobby.rs` is `TableBuilder` + widgets.
   With Slint the theme would live half in `.slint` markup and half in Rust, which fits the spec's
   module split less cleanly.

### Known risks, stated plainly

* **OpenGL requirement — happened, and is now closed.** The `glow` renderer needs `OPENGL32.dll`
  to resolve a real driver. On a Windows VM with no GPU driver, Microsoft's software GL is 1.1 and
  eframe fails to create a context. This is not hypothetical: it is what the client did the first
  time somebody ran it in a VM, and the message it printed then — *"run with --headless if this
  machine has no display"* — was advice about a different problem.

  **Closed 2026-08-29**, by the mitigation this section proposed: `eframe`'s `wgpu` renderer is
  compiled in beside `glow`, and the client re-launches itself on it when OpenGL is missing.
  Three things were measured that this section had not been:

  1. **Only the `dx12` backend is needed, and only it can be afforded.** `egui-wgpu` takes `wgpu`
     with `default-features = false`, so the backend set is ours to choose. Turning them all on
     (via `eframe`'s own `wgpu` feature) puts `wgpu-hal`'s dx12 and vulkan code in the build at
     once and the `windows` crate resolves to two incompatible versions — `gpu-allocator 0.28.0`
     accepts `>=0.53, <=0.62` and unifies onto the `0.57` that `sysinfo` pulls in under
     `libp2p-memory-connection-limits`, while `wgpu-hal` itself is written against `0.62`.
     `error[E0308]: there are multiple different versions of crate windows`. Asking for `dx12`
     alone resolves cleanly on `windows 0.62.2`. **The memory-limit defence was never a candidate
     for removal to make a renderer build.**
  2. **WARP is there.** `Microsoft Basic Render Driver` is enumerated as a `Cpu` adapter next to
     this machine's two real GPUs, driver version `10.0.19041` — the Windows build number, not a
     driver's — and `wgpu-hal`'s dx12 backend knows it by name. Nothing to install.
  3. **The retry must be a second process.** `winit 0.30.13` swaps a process-global `AtomicBool`
     the first time an event loop is built and never clears it (`event_loop.rs:119`), so a
     fallback inside the same process gets `EventLoopError::RecreationAttempt` and nothing else.

  Cost: 21.3 MB → 26.4 MB, 23 crates, two new names in the PE import table (`dxgi.dll`,
  `setupapi.dll`, both Windows' own), and no new licence expression in the tree.
  Slint has the same exposure with `femtovg`, plus a `renderer-software` option we did not
  measure — moot now.
* **egui is immediate-mode.** The whole table is redrawn every frame. For one poker table that is
  irrelevant, but the table window should be repaint-throttled when idle (`ctx.request_repaint_after`)
  so a laptop is not pinned at the display refresh rate.
* **Rapid API churn.** 0.36 renamed `Rounding` → `CornerRadius` and replaced `App::update` with
  `App::ui`. Pin `=0.36.1` in `Cargo.toml` and commit `Cargo.lock` (§28).
* **`+crt-static` is mandatory and easy to lose.** Put it in `.cargo/config.toml` in the repo, not
  in a shell variable, or the release build silently starts requiring VC++ redistributables again.
