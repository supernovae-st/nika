# Matrix Rain Effect v2 - Design Plan

## Overview

Redesign the Matrix Decrypt streaming effect with a "Matrix Rain + Wave + Sparks" combo for a more polished, animated experience.

## Current Problems

1. **Spaces not preserved** - chaos replaces spaces, losing text structure
2. **Double display** - "Thinking..." + streaming effect shown simultaneously
3. **Too many emojis** - effect feels chaotic, not Matrix-style
4. **No falling animation** - characters appear in place, no motion

## Design Goals

- Characters fall from above like Matrix rain
- Land and reveal left-to-right with sparks on impact
- Preserve spaces (see text structure during chaos)
- 2-3 words in chaos max (wave moyenne)
- Clean white text when revealed
- Colorful rain (Solarized palette)

## Visual Reference

```
Frame 1:     ｱ         ｶ        ﾊ
             ｲ         ｷ        ﾋ      ﾗ
             ｳ         ｸ        ﾌ      ﾘ
          ·  ▼         ▼        ▼      ▼
─────────────────────────────────────────
 H  e  l  l  o     w  o  r  l  d  ,  _  _

Frame N:
─────────────────────────────────────────
 Hello world, how are you today?  ← all white
```

## Architecture

### New Widget: `MatrixRainReveal`

```
src/tui/widgets/
├── matrix_rain_reveal.rs   ← NEW: Combined rain + reveal effect
├── matrix_decrypt.rs       ← KEEP: For reference, deprecate later
└── mod.rs                  ← Export new widget
```

### Components

```
MatrixRainReveal
├── RainColumn[]        - Falling character columns
├── LandingZone         - Where text appears
├── SparkManager        - Impact particle effects
└── RevealWave          - Left-to-right white reveal
```

## Data Structures

```rust
/// Single falling character
struct RainDrop {
    char: char,           // ｱｲｳ or a-z
    color: Color,         // Solarized
    y: f32,               // Vertical position (0.0 = top, 1.0 = landed)
    speed: f32,           // Fall speed
    column: usize,        // Which text column
}

/// Spark particle on impact
struct Spark {
    char: char,           // · ˚ * ✦
    x: u16,
    y: u16,
    life: u8,             // Frames remaining
    color: Color,
}

/// Main widget state
pub struct MatrixRainReveal {
    text: String,                    // Full text to reveal
    revealed_chars: usize,           // How many chars are white
    rain_columns: Vec<Vec<RainDrop>>,// Rain per text column
    sparks: Vec<Spark>,              // Active sparks
    rain_height: u8,                 // Lines above text (3-4)
    frame: u64,                      // Animation frame
    seed: u64,                       // RNG seed
}
```

## Animation Logic

### Phase 1: Rain Generation
- When new token arrives, create rain columns for each char
- Each column has 3-5 drops falling at different speeds
- Preserve spaces (no rain on space columns)

### Phase 2: Falling
- Drops fall 1 row per frame (adjustable speed)
- Colors: cyan, green, yellow, orange, magenta, violet (random)
- Characters: 45% katakana, 35% ascii, 10% kanji, 8% nerd, 2% emoji

### Phase 3: Landing + Sparks
- When drop reaches y=1.0, trigger landing
- Spawn 1-2 sparks (· ˚ * ✦) with 2-3 frame lifetime
- Sparks are yellow/white BRIGHT

### Phase 4: Reveal Wave
- Reveal wave follows 2-3 words behind new tokens
- Revealed chars turn WHITE
- Wave speed: ~1 char per frame

## Color Palette

```rust
// Rain (falling) - DIM modifier
const RAIN_COLORS: &[Color] = &[
    solarized::CYAN,
    solarized::GREEN,
    solarized::YELLOW,
    solarized::ORANGE,
    solarized::MAGENTA,
    solarized::VIOLET,
];

// Sparks - BRIGHT modifier
const SPARK_CHARS: &[char] = &['·', '˚', '*', '✦'];
const SPARK_COLORS: &[Color] = &[solarized::YELLOW, solarized::BASE3]; // white

// Revealed text
const REVEALED_COLOR: Color = solarized::BASE3; // white
```

## Character Distribution (Rain)

```
45% Half-width katakana  ｱｲｳｴｵｶｷｸ...
35% ASCII                a-z, 0-9, symbols
10% Kanji                自由海賊風雷...
 8% Nerd refs            42, NULL, sudo...
 2% Nika mascots         🐔⚡📟🛰️🔌🦋
```

## Integration with ChatView

```rust
// In chat.rs
pub struct ChatView {
    // Replace streaming_decrypt with:
    rain_reveal: MatrixRainReveal,
    // ...
}

impl ChatView {
    pub fn append_streaming(&mut self, chunk: &str) {
        // Add text to rain_reveal
        self.rain_reveal.push_text(chunk);
    }

    pub fn tick_animation(&mut self) {
        // Called every frame (60fps)
        self.rain_reveal.tick();
    }
}
```

## Render Layout

```
┌─────────────────────────────────────────┐
│  [rain zone - 3-4 lines]                │  ← falling chars
│     ｱ    ｶ    ﾊ                         │
│     ｲ    ｷ    ﾋ    ﾗ                    │
│     ▼    ▼    ▼    ▼                    │
├─────────────────────────────────────────┤
│  Hello world, how ｱｲｳ ｶｷ  ← text zone   │  ← landing zone
└─────────────────────────────────────────┘
```

## Implementation Steps

### Step 1: Create MatrixRainReveal struct
- [ ] Define RainDrop, Spark, MatrixRainReveal structs
- [ ] Implement Default, new(), basic methods
- [ ] Unit tests for struct creation

### Step 2: Character generation
- [ ] Implement random_glyph() with new distribution
- [ ] Preserve spaces (skip rain on spaces)
- [ ] Unit tests for distribution

### Step 3: Rain animation
- [ ] Implement tick() for falling animation
- [ ] Rain column management (add/remove drops)
- [ ] Unit tests for animation

### Step 4: Spark system
- [ ] Implement spark spawning on landing
- [ ] Spark lifecycle (spawn, fade, remove)
- [ ] Unit tests for sparks

### Step 5: Reveal wave
- [ ] Implement wave progression
- [ ] Track revealed_chars
- [ ] Unit tests for reveal

### Step 6: Widget render
- [ ] Implement Widget trait
- [ ] Render rain zone (above text)
- [ ] Render landing zone (text line)
- [ ] Render sparks overlay
- [ ] Visual tests

### Step 7: Integration
- [ ] Replace streaming_decrypt in ChatView
- [ ] Wire up append_streaming()
- [ ] Wire up tick_animation()
- [ ] Remove double display bug
- [ ] Integration tests

### Step 8: Polish
- [ ] Tune timing parameters
- [ ] Test with real streaming
- [ ] Performance check (60fps)

## Files to Modify

| File | Changes |
|------|---------|
| `src/tui/widgets/mod.rs` | Export MatrixRainReveal |
| `src/tui/widgets/matrix_rain_reveal.rs` | NEW - main widget |
| `src/tui/views/chat.rs` | Replace streaming_decrypt |
| `src/tui/app.rs` | Remove double display |

## Success Criteria

- [ ] Rain falls visibly (3-4 lines above text)
- [ ] Sparks appear on impact
- [ ] Text reveals white left-to-right
- [ ] Spaces preserved (structure visible)
- [ ] No double display
- [ ] 60fps smooth animation
- [ ] Colors match Solarized palette
