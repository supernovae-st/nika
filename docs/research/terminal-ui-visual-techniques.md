# Research Report: Terminal UI Visual Techniques -- The Absolute Cutting Edge

## Summary

This report catalogs the most visually stunning terminal UIs, sci-fi interface design patterns, and advanced terminal graphics techniques ever created. It covers real applications (eDEX-UI, cool-retro-term, btm, k9s), Hollywood FUI design (JARVIS, Tron Legacy, Blade Runner), Unicode-based graphics (Braille, block elements, sextants), and pixel-level terminal protocols (Kitty graphics, Sixel). The goal: identify every technique available for building the most advanced terminal UI possible.

---

## 1. eDEX-UI -- The Gold Standard of Sci-Fi Terminals

**Repository**: https://github.com/GitSquared/edex-ui
**Status**: Maintenance mode (no active development), last major version ~2.2
**Stack**: Electron, JavaScript (87.8%), CSS (11.5%), xterm.js, SmoothieCharts, systeminformation

### Panel Layout (Fullscreen HUD)

```
+------------------+-----------------------------+------------------+
|                  |                             |                  |
|  SYSTEM MONITOR  |     CENTRAL TERMINAL        |  NETWORK MONITOR |
|  - CPU usage     |     (xterm.js emulator)     |  - GeoIP globe   |
|  - CPU temp      |     - Multi-tab (5+)        |  - Active conns  |
|  - RAM/swap      |     - Full curses support   |  - Transfer rates|
|  - Processes     |     - Mouse events          |  - Bandwidth     |
|  - Uptime        |     - Colors                |  - IP info       |
|  - OS details    |                             |  - ENCOM Globe   |
|                  |                             |                  |
+------------------+-----------------------------+------------------+
|  FILE BROWSER    |    ON-SCREEN KEYBOARD                          |
|  (follows CWD)   |    (QWERTY, touch-enabled, mirrors typing)    |
|  (clickable nav) |    (customizable layouts)                      |
+------------------+-----------------------------------------------+
```

### What Makes It Feel Sci-Fi

| Effect | Implementation |
|--------|---------------|
| Glowing neon themes | CSS variables for colors, fonts, cursor styles |
| Real-time animated graphs | SmoothieCharts library (smooth line interpolation) |
| Spinning 3D globe | ENCOM Globe by Rob Scanlon (WebGL) |
| Dynamic stats overlays | systeminformation library polling |
| Particle/glitch effects | CSS animations + JS canvas overlays |
| Sound effects | Optional typing beeps ("Hollywood hacking vibe") |
| Fullscreen immersion | No window chrome, cockpit-like layout |

### Built-in Themes
- `tron` -- Blue neon on black (TRON: Legacy inspired)
- `tron-notype` -- Tron without keyboard visualization
- `blade` -- Warm amber/orange (Blade Runner inspired)
- `horizon` -- Modern colorful (community theme by GitSquared)
- `red-notype-disrupted-color-tty` -- Red cyberpunk variant

### Community Theme System
- Themes are `.json` files in `~/.config/eDEX-UI/themes/`
- Define: fonts, foreground/background colors, cursor style, accent colors
- Advanced: CSS injection for layout modifications
- Switch via Ctrl+Shift+S settings menu, auto-reload
- Community repos: `GitSquared/horizon-edex-theme`, `M0n7y5/eDEX-UI-Custom-Themes`
- Inspirations reported: Tron, Evangelion, Cyberpunk 2077, Matrix (custom creations)

---

## 2. cool-retro-term -- CRT Shader Perfection

**Repository**: https://github.com/Swordfish90/cool-retro-term
**Stack**: Qt/QML with OpenGL shaders

### Shader Effects Breakdown

| Effect | What It Does | CRT Physics |
|--------|-------------|-------------|
| **Scanlines** | Horizontal lines simulating electron beam raster scanning | Beam draws left-to-right, top-to-bottom |
| **Bloom** | Light halation around bright text, soft glow | Phosphor glow bleeds into adjacent areas |
| **Phosphor persistence** | Ghost trails on moving text, afterglow | Phosphors retain energy briefly after excitation |
| **Screen curvature** | Barrel distortion with rounded black edges | CRT glass is physically curved (spherical tube) |
| **Jitter** | Subtle positional noise, vibration | Electromagnetic interference on electron beam |
| **Static/noise** | Random dot patterns, interference lines | Signal degradation, poor shielding |
| **Burn-in** | Persistent ghost of static content | Permanent phosphor damage from constant display |
| **Flicker** | Periodic brightness variation | 50/60Hz refresh rate visible on slow phosphors |
| **Ambient glow** | Soft light emission around the screen area | Light scattering through CRT glass |
| **Color bleed** | Colors leaking into adjacent pixels | Shadow mask/aperture grille imprecision |

### Configurable Parameters
All accessible via GUI sliders or `~/.config/cool-retro-term/config`:
- `burnin` -- Intensity of burn-in simulation (0.0 to 1.0)
- `flicker` -- Flicker rate and amplitude
- `bloom` -- Glow radius and intensity
- `static` -- Noise pattern density
- `curvature` -- Barrel distortion radius
- Scanline density/thickness
- Phosphor glow/halo radius
- Jitter amplitude (X and Y)
- Color bleed amount
- Gamma, brightness, contrast
- Font selection (period-appropriate monospace fonts)

### What Makes It Authentic
The key insight: real CRT appearance comes from **emulating phosphor physics**, not adding blur.
- Phosphors blend at viewing distance (natural halation)
- Scanline gaps create apparent softness without actual blur
- Time-varying noise replicates beam instability
- Curvature hides edge artifacts naturally

---

## 3. Sci-Fi Movie UI Design Patterns (FUI -- Fantasy User Interface)

### The Canon Films and Their Signatures

#### Iron Man -- JARVIS/FRIDAY (Territory Studio, Prologue Films)
- **Holographic projections**: Floating 3D wireframes in physical space
- **Transparent overlays**: Semi-transparent data layers over real-world view
- **Color coding**: Cyan (friendly/normal) shifting to red (threat/alert)
- **Gesture interaction**: Pinch, swipe, throw gestures on holograms
- **Radial menus**: Circular option selectors around focal points
- **Data density**: Dense information with minimal text, heavy on graphs

#### Tron: Legacy -- GMUNK / Bradley Munkowitz
- **Neon line art**: Thin glowing lines on pure black
- **Grid systems**: Infinite perspective grids (the iconic "digital world" floor)
- **Geometric precision**: Perfect circles, hexagons, angular typography
- **Monochromatic palette**: White/cyan/orange on black
- **Light trails**: Motion paths that persist and fade
- **Particle systems**: Derezzed/materialization effects

#### Minority Report -- Mark Canter / Adaptive Path
- **Gesture-driven manipulation**: Full-body gestural control of data
- **Data scrubbing**: Timeline-based interaction with video evidence
- **Translucent panels**: Layered information on glass surfaces
- **Blue/white palette**: Clinical, institutional color scheme
- **Cascading data**: Information flows and sorts spatially

#### Ghost in the Shell
- **Terminal/hacker aesthetic**: Green/amber text on black
- **Data rain**: Cascading character streams (Matrix-adjacent)
- **Glitch effects**: Visual corruption as metaphor for hacking
- **Augmented overlays**: Information embedded in character vision
- **Wireframe models**: Low-poly 3D overlays on real objects

#### Blade Runner 2049 -- Territory Studio
- **Warm dystopian palette**: Orange, amber, desaturated tones
- **Lo-fi interfaces**: Deliberately low-resolution, grainy, imperfect
- **E-ink aesthetics**: Muted, low-power display simulation
- **Environmental integration**: UIs embedded in architecture
- **Brutalist typography**: Heavy, utilitarian font choices

### Universal FUI Design Patterns

```
PATTERN                     TERMINAL EQUIVALENT
---------------------------------------------------------
Circular HUDs/gauges     -> Unicode arc characters + Braille dots
Radial menus             -> Centered text with directional indicators
Hexagonal grids          -> Box drawing with angled connections
Data rain/streams        -> Scrolling text columns with color fade
Wireframe globes         -> ASCII/Braille dot sphere rendering
Parallax depth layers    -> Z-ordered overlapping panels
Glitch/corruption        -> Random character substitution + color noise
Chromatic aberration     -> Offset colored text shadows (R/G/B shift)
Neon glow                -> Bold + bright color on dim background
Holographic transparency -> Dim background text showing through panels
Particle systems         -> Sparse Braille dots moving per frame
Scan lines              -> Alternating dim/bright rows
```

### Studios and Artists to Study
- **Territory Studio**: Guardians of the Galaxy, Blade Runner 2049, Avengers
- **GMUNK (Bradley Munkowitz)**: Tron: Legacy, Oblivion
- **Ash Thorp**: Total Recall (2012), intricate HUD details
- **BLIND**: Star Wars: The Force Awakens
- **Spov Design**: Various sci-fi productions
- **Perception**: Iron Man, Avengers holographic interfaces
- Reference site: https://scifiinterfaces.com

---

## 4. Real Terminal Apps with Exceptional Design

### System Monitors

#### bottom (btm)
- Braille dot graphics for CPU/memory sparklines
- 24-bit true color gradient fills
- Responsive layout adapting to terminal size
- Smooth real-time graph updates via differential rendering

#### btop++
- Rich Unicode box drawing for panel borders
- Color gradients across CPU core bars
- Process tree visualization
- Smooth animations for graph scrolling
- One of the most visually polished system monitors

#### sampler
- Real-time sparklines and Braille graphics
- YAML-configured dashboard layout
- Smooth scrolling data streams
- Multiple visualization types: bars, gauges, text logs

### DevOps Tools

#### lazydocker
- Tabbed grid-based container views
- Sparklines for resource history
- Color-coded health indicators (green to red gradient)
- Responsive panel reflow on resize

#### k9s
- Navigable pod/container tree views
- Color-coded severity levels with true color
- Blinking animations for warnings/errors
- Real-time log streaming with syntax highlighting

#### wtfutil
- Tiled dashboard with heterogeneous widgets
- Sparklines for metric timelines
- Heatmap-style color mapping
- Multi-source data aggregation

### Creative/Fun Terminal Projects

| Project | Visual Effect |
|---------|--------------|
| **no-more-secrets** | Sneakers (1992) decryption animation -- text scrambles then reveals |
| **cmatrix** | Matrix digital rain -- cascading green katakana characters |
| **pipes.sh** | Animated colorful growing pipes filling the screen |
| **cbonsai** | Real-time growing ASCII bonsai tree |
| **Browsh** | Full web browser rendered in terminal (via Kitty/iTerm2 images) |
| **gif-for-cli** | GIF to ASCII animation converter |
| **Grafterm** | Grafana-style animated metrics dashboards |
| **Terminal Doom** | Actual Doom running via ASCII/Unicode rendering |

---

## 5. Terminal Art -- Unicode Graphics Techniques

### Resolution Hierarchy (Pixels Per Character Cell)

```
TECHNIQUE          GRID     BITS/CELL  EFFECTIVE RESOLUTION (80x24 terminal)
---------------------------------------------------------------------------
Full blocks        1x1      1          80 x 24
Half blocks        1x2      2          80 x 48
Quarter blocks     2x2      4          160 x 48
Sextant chars      2x3      6          160 x 72
Braille patterns   2x4      8          160 x 96
1/8th blocks       1x8      8          80 x 192
```

### Unicode Block Ranges

#### Box Drawing (U+2500 -- U+257F)
```
Single:  --- | +-- --+ |-- --| -+- +-+ -++ +-- --+ +++ +-+ -++ ++- +-+
Double:  === ! +== ==+ !== ==! =+= +=+ =++ +== ==+ +++ +=+ =++ ++= +=+
Heavy:   === | +== ==+ !== ==! =+= +=+ ...
Dashed:  - - | . . +- -+ |- -| ...

Examples:
  +---+---+     +===+===+     +---+===+
  |   |   |     !   !   !     |   !   !
  +---+---+     +===+===+     +---+===+
```

#### Block Elements (U+2580 -- U+259F)
```
Full:    [U+2588]
Halves:  [U+2580] upper   [U+2584] lower   [U+258C] left   [U+2590] right
Quarters: [U+2596] lower-left  [U+2597] lower-right
          [U+2598] upper-left  [U+259D] upper-right
          [U+2599] all-but-upper-right  [U+259B] all-but-lower-right
          [U+259C] all-but-lower-left   [U+259F] all-but-upper-left
Shades:  [U+2591] light  [U+2592] medium  [U+2593] dark

Trick: Combine upper-half-block with different fg/bg colors
       to get 2 independent pixels per cell vertically.
```

#### Braille Patterns (U+2800 -- U+28FF)
```
Dot positions in a 2x4 grid:
  [1] [4]       Bit mapping:
  [2] [5]       char = U+2800 + (dot1<<0 | dot2<<1 | dot3<<2 |
  [3] [6]                        dot4<<3 | dot5<<4 | dot6<<5 |
  [7] [8]                        dot7<<6 | dot8<<7)

  Examples:
  U+2800 = empty     (no dots)
  U+2801 = dot 1     (top-left only)
  U+28FF = all dots  (fully filled)
  U+2847 = dots 1,2,3,7 (left column)

  256 possible patterns (2^8)
```

### Color Enhancement Techniques

```
24-BIT TRUE COLOR (RGB)
  Foreground: \x1b[38;2;R;G;Bm
  Background: \x1b[48;2;R;G;Bm

  Combined with half-blocks:
    Each cell = 2 pixels vertically
    Top pixel = foreground color (on upper-half-block)
    Bottom pixel = background color
    Result: 160x48 pixels at full RGB color in 80x24 terminal

GRADIENT TECHNIQUE:
  For each column, interpolate between two colors:
    for x in 0..width:
      r = r1 + (r2-r1) * x / width
      g = g1 + (g2-g1) * x / width
      b = b1 + (b2-b1) * x / width
      print "\x1b[38;2;{r};{g};{b}m[block char]"
```

### Practical Maximum Resolution

On an 80x24 terminal with Braille patterns + true color:
- **Geometric resolution**: 160 x 96 points (Braille 2x4 grid)
- **Color resolution**: 16.7M colors per dot (but only 2 colors per cell -- fg/bg)
- **Half-block with true color**: 80 x 48 pixels at full RGB (best color fidelity)
- **Best combined**: Braille for shape + true color for shading = highest detail

---

## 6. Advanced Terminal Graphics Protocols

### Kitty Graphics Protocol (The Most Advanced)

**Spec**: https://sw.kovidgoyal.net/kitty/graphics-protocol/
**Support**: Kitty, Ghostty, WezTerm, Konsole, st (patch)

| Feature | Detail |
|---------|--------|
| Image formats | PNG, RGB (24-bit), RGBA (32-bit) |
| Transmission | Direct, shared memory, filesystem, chunked |
| Compression | ZLIB deflate |
| Animation | Frame-based with timing control |
| Transparency | Full alpha channel |
| Placement | Pixel-precise positioning within cells |
| Query | Terminal capability detection |
| Unicode placeholders | Virtual placement via Unicode chars |

```
Protocol: OSC-based escape sequences
  Transmit image:  \x1b_Gf=32,s=<width>,v=<height>,a=T;<base64 data>\x1b\\
  Display cached:  \x1b_Ga=p,i=<image_id>\x1b\\
  Animate:         \x1b_Ga=f,i=<image_id>,z=<frame>\x1b\\
```

### Sixel Graphics Protocol (The Most Compatible)

**Origin**: DEC VT240 (1983)
**Support**: xterm, mlterm, WezTerm, foot, tmux, Zellij

| Feature | Detail |
|---------|--------|
| Encoding | 6 vertical pixels per character (hence "sixel") |
| Colors | Register-based (RGB or HLS), typically 256 registers |
| Transmission | Inline escape sequences (7-bit safe) |
| Compression | Run-length encoding |
| tmux support | YES (major advantage over Kitty protocol) |
| Zellij support | YES |

```
Key advantage: Works inside tmux and Zellij
Key limitation: Lower performance than Kitty, older encoding
```

### Protocol Comparison

```
                    Kitty Graphics    Sixel           iTerm2 Inline
--------------------------------------------------------------------
Max colors          16.7M (32-bit)   256 registers   16.7M
Transparency        Yes (alpha)       No              Yes
Animation           Native frames     Manual          GIF support
tmux support        NO                YES             YES (partial)
Compression         ZLIB              RLE             Base64 only
Adoption            Growing fast      Established     iTerm2 only
Performance         Excellent         Good            Good
Multiplexer compat  Poor              Excellent       Partial
```

---

## 7. Modern Terminal Emulator Capabilities

### Feature Matrix (2025 State of the Art)

| Feature | Ghostty | Kitty | WezTerm | Alacritty |
|---------|---------|-------|---------|-----------|
| GPU acceleration | Metal/OpenGL | OpenGL | OpenGL/Vulkan | OpenGL |
| Kitty graphics | Yes | Yes (origin) | Yes | No |
| Sixel | Planned | Yes | Yes | No |
| True color (24-bit) | Yes | Yes | Yes | Yes |
| Ligatures | Yes (native) | Yes | Yes | No |
| Undercurl | Yes | Yes | Yes | Yes |
| Colored underlines | Yes | Yes | Yes | Yes |
| Kitty keyboard protocol | Yes | Yes (origin) | Yes | No |
| Synchronized rendering | Yes | Yes | Yes | No |
| Font shaping | Native engine | HarfBuzz | HarfBuzz | N/A |

### Synchronized Rendering (Flicker Prevention)

```
Begin sync:  \x1b[?2026h   (BSU - Begin Synchronized Update)
[render all frame content here]
End sync:    \x1b[?2026l   (ESU - End Synchronized Update)

Result: Terminal buffers all output between BSU/ESU,
        then renders entire frame atomically.
        Eliminates tearing and partial-frame artifacts.
```

---

## 8. Animation and Rendering Techniques

### The Three-Layer Flicker-Free Stack

```
Layer 1: DOUBLE BUFFERING
  - Render next frame to off-screen buffer
  - Swap/diff against visible buffer atomically
  - Eliminates tearing during updates

Layer 2: DIFFERENTIAL RENDERING
  - Compare previous frame with current frame cell-by-cell
  - Only emit ANSI sequences for changed cells
  - Reduces bandwidth by 90%+ for typical UI updates

Layer 3: SYNCHRONIZED OUTPUT
  - Wrap frame output in BSU/ESU escape sequences
  - Terminal holds display until frame is complete
  - Prevents partial rendering artifacts
```

### Frame Rate Guidelines

| Scenario | Target FPS | Technique |
|----------|-----------|-----------|
| Idle UI (no animation) | 1-4 | Event-driven updates only |
| Active graphs/sparklines | 10-15 | Timer-based polling + diff render |
| Smooth animations (easing) | 30-60 | RequestAnimationFrame-style loop |
| Data streaming/logs | 10-30 | Batched updates with throttling |

### Easing Functions for Terminal Animation

```
Linear:      t
Ease-in:     t * t
Ease-out:    t * (2 - t)
Ease-in-out: t < 0.5 ? 2*t*t : -1+(4-2*t)*t
Bounce:      Custom piecewise function

Applied to: panel slides, fade-in/out (via color interpolation),
            progress bars, selection highlights, scroll momentum
```

---

## 9. Chromatic Aberration and Glitch Effects in Terminal

### Simulating Chromatic Aberration (RGB Split)

```
Normal text:   "SYSTEM ONLINE"

With chromatic aberration (3 offset layers):
  Red channel:   \x1b[31m  SYSTEM ONLINE      (offset -1 col)
  Green channel: \x1b[32m   SYSTEM ONLINE     (offset  0 col, base)
  Blue channel:  \x1b[34m    SYSTEM ONLINE    (offset +1 col)

  Overlapped on same row using cursor repositioning:
  \x1b[{row};{col-1}H\x1b[31mSYSTEM ONLINE
  \x1b[{row};{col}H\x1b[32mSYSTEM ONLINE
  \x1b[{row};{col+1}H\x1b[34mSYSTEM ONLINE
```

### Glitch Effect Techniques

```
1. CHARACTER SUBSTITUTION
   Replace random characters with Unicode box-drawing/block elements
   "SYSTEM ONLINE" -> "SYS+EM [N|INE"

2. COLOR NOISE
   Randomly shift fg/bg colors on individual characters
   Apply for 1-3 frames, then restore

3. ROW DISPLACEMENT
   Shift entire rows left or right by 1-3 characters
   Creates "tearing" effect

4. SCANLINE CORRUPTION
   Dim or brighten alternating rows
   Insert blank rows momentarily

5. DATA RAIN OVERLAY
   Sparse random characters falling through content
   Use dim green on existing text background
```

---

## 10. Putting It All Together -- Maximum Visual Impact Checklist

### Tier 1: Essential (Works Everywhere)

- [ ] 24-bit true color for gradients and theming
- [ ] Unicode box drawing for clean panel borders
- [ ] Bold + dim text for depth hierarchy
- [ ] Sparklines using Braille patterns (U+2800 block)
- [ ] Half-block characters for 2x vertical resolution
- [ ] Synchronized rendering (BSU/ESU) for flicker-free updates
- [ ] Differential rendering for performance

### Tier 2: Advanced (Modern Terminals)

- [ ] Braille dot graphics for high-res charts/graphs
- [ ] Color gradients on progress bars and gauges
- [ ] Smooth easing animations (panel transitions, highlights)
- [ ] Chromatic aberration text effects (RGB offset)
- [ ] Glitch/corruption effects (character substitution + color noise)
- [ ] Alternating row brightness (scanline simulation)
- [ ] Dim text for "holographic" panel transparency effect
- [ ] Colored/styled underlines (undercurl for warnings)

### Tier 3: Cutting Edge (Protocol-Dependent)

- [ ] Kitty graphics protocol for inline images/icons
- [ ] Sixel fallback for tmux compatibility
- [ ] Animated frames via Kitty animation protocol
- [ ] Unicode sextant characters for 3x2 sub-cell resolution
- [ ] GPU-accelerated terminal for 60fps rendering
- [ ] Pixel-precise mouse interaction (Kitty mouse protocol)
- [ ] Custom font rendering with ligatures

### Tier 4: The Absolute Frontier

- [ ] Real-time Braille-dot particle systems
- [ ] Wireframe 3D object rotation using Braille canvas
- [ ] Terminal ray tracing with half-block pixels + true color
- [ ] Procedural animation (flocking, physics simulation)
- [ ] Audio-reactive visualizations (pipe audio data to visual)
- [ ] Multi-layer compositing (foreground data + dim background matrix)
- [ ] CRT post-processing simulation (scanlines + bloom via color math)

---

## Sources

1. [eDEX-UI GitHub](https://github.com/GitSquared/edex-ui) -- Main repository, features, architecture
2. [eDEX-UI on It's FOSS](https://itsfoss.com/edex-ui-sci-fi-terminal/) -- Feature overview and screenshots
3. [cool-retro-term GitHub](https://github.com/Swordfish90/cool-retro-term) -- CRT shader parameters
4. [cool-retro-term on Sudo Science](https://sudoscience.blog/2023/12/05/cool-retro-term-lives-up-to-its-name/) -- Effect breakdown
5. [Kitty Graphics Protocol Spec](https://sw.kovidgoyal.net/kitty/graphics-protocol/) -- Official protocol documentation
6. [Sixel on Wikipedia](https://en.wikipedia.org/wiki/Sixel) -- Protocol history and encoding
7. [libsixel](https://saitoha.github.io/libsixel/) -- Sixel encoder/decoder library
8. [Ghostty Features](https://ghostty.org/docs/features) -- Terminal emulator capabilities
9. [Ghostty State of Terminals 2025](https://github.com/ghostty-org/ghostty/discussions/9466) -- Benchmark comparison
10. [scifiinterfaces.com](https://scifiinterfaces.com) -- Sci-fi UI analysis database
11. [SitePoint FUI Guide](https://www.sitepoint.com/14-top-sci-fi-designs-to-inspire-your-next-interface/) -- Design pattern catalog
12. [Unicode Box Drawing (Wikipedia)](https://en.wikipedia.org/wiki/Box-drawing_characters) -- Character reference
13. [Unicode Terminals Proposal L2/17-435R](https://www.unicode.org/L2/L2017/17435r-terminals-prop.pdf) -- Sextant/mosaic chars
14. [Textual Smooth Scrolling](https://textual.textualize.io/blog/2025/02/16/smoother-scrolling-in-the-terminal-mdash-a-feature-decades-in-the-making/) -- Pixel-precise scrolling
15. [Talk Python: Terminal App Algorithms](https://talkpython.fm/episodes/show/498/algorithms-for-high-performance-terminal-apps) -- Differential rendering
16. [Ratatui Widget Showcase](https://ratatui.rs/showcase/widgets/) -- Canvas, sparkline, chart widgets
17. [awesome-tuis](https://github.com/rothgar/awesome-tuis) -- Comprehensive TUI project list
18. [Horizon eDEX Theme](https://github.com/GitSquared/horizon-edex-theme) -- Community theme example
19. [Sixel in Ghostty Discussion](https://github.com/ghostty-org/ghostty/discussions/2496) -- Protocol comparison
20. [CRT Shader Breakdown (Cyan)](https://cyangamedev.wordpress.com/2020/09/10/retro-crt-shader-breakdown/) -- Shader effect analysis

## Methodology

- **Tools used**: Perplexity AI search (12 queries), cross-referenced across sources
- **Pages analyzed**: 40+ sources across GitHub repos, documentation, blog posts, discussions
- **Time period covered**: Projects from 2013 (cool-retro-term) through March 2026

## Confidence Level

**High** -- All major techniques are well-documented in official specs and open-source repositories. The protocol capabilities (Kitty, Sixel) are drawn from official documentation. The FUI design patterns are sourced from professional design studios with public portfolios.

## Key Takeaway for Nika TUI

The most impactful techniques that work within a Ratatui-based Rust TUI (no Electron/WebGL):

1. **Braille dot canvas** for high-resolution charts and data visualizations
2. **Half-block pixels + true color** for maximum color fidelity at 2x resolution
3. **Synchronized rendering** to eliminate all flicker
4. **Differential rendering** (Ratatui does this natively)
5. **24-bit color gradients** for sci-fi aesthetic theming
6. **Chromatic aberration** via ANSI cursor repositioning for accent effects
7. **Glitch effects** via periodic character/color substitution
8. **Easing-based animations** for panel transitions and selection highlights
9. **Dim text layering** for depth (background data visible through panels)
10. **Sparklines everywhere** -- the single highest-impact data visualization technique
