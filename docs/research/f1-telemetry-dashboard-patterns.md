# Research Report: F1 Telemetry Dashboard Patterns for TUI Design

## Summary

Formula 1 telemetry systems represent the most sophisticated real-time data visualization in sports, processing 300+ sensors per car at rates exceeding 1M data points per second across the grid. This document catalogs concrete visual patterns, color systems, layout architectures, and chart types used across the F1 ecosystem -- from pit wall screens to steering wheel displays to broadcast graphics -- as design vocabulary for the Nika TUI.

## 1. Pit Wall Telemetry Screens

### 1.1 Scale of Data

| Metric | Value |
|--------|-------|
| Sensors per car | 300+ (250+ physical, additional derived) |
| CAN buses per car | 17 |
| Data per lap (live) | ~30 MB |
| Data per lap (full, post-umbilical) | ~60-90 MB |
| Data per race (per car) | ~1.8-5.4 GB |
| Data points per second (grid) | ~1.1 million |
| Sampling rate (key channels) | 100-1000 Hz |
| Transmission latency (Europe) | ~10 ms |
| Transmission latency (flyaway) | up to 300 ms |

### 1.2 Data Channels (Per Car)

**Engine / Power Unit:**
- Engine RPM (0-15,000+)
- Exhaust temperature
- Oil pressure, oil temperature
- Water/coolant temperature
- Fuel flow rate (kg/h)
- Fuel remaining (kg)
- Turbo RPM and boost pressure

**ERS (Energy Recovery System):**
- MGU-K power output (kW)
- MGU-H power output (kW)
- Energy store charge level (kJ, 0-4000)
- Deployment mode (Harvest / Deploy / Off)
- Battery temperature

**Chassis / Dynamics:**
- Speed (km/h, 0-370)
- G-force (longitudinal, lateral, vertical -- 3 axes)
- Ride height (front, rear)
- Suspension travel (4 corners)
- Hydraulic pressure (multiple points)

**Driver Inputs:**
- Throttle position (0-100%)
- Brake pressure (bar)
- Steering angle (degrees)
- Gear position (1-8 + N/R)
- DRS flap position (open/closed)
- Clutch position (2 paddles)

**Tires:**
- Surface temperature (4 tires x 3 zones: inner/middle/outer)
- Carcass temperature (4 tires)
- Tire pressure (4 tires)
- Tire wear estimate (derived)

**Brakes:**
- Brake disc temperature (4 corners)
- Brake pad wear
- Brake balance (front/rear %)

**Aerodynamics:**
- Front wing flap position
- Rear wing DRS actuator state
- Pitot tube airspeed

### 1.3 Monitor Layout (Per Engineer)

Typical pit wall engineer has **3 screens**:

```
+----------------------------------+
|  TOP: FOM Broadcast + 3 Timing  |  <-- Live TV feed + timing pages
|        Pages (tiled)             |
+----------------------------------+
|  MID: Strategy Data + Track Map |  <-- Real-time car positions
|        + Gap Analysis            |
+----------------------------------+
|  BOT: Telemetry Traces          |  <-- Sensitive live data
|        (not shown on camera)     |
+----------------------------------+
```

**Screen content by role:**

| Role | Primary Display | Secondary | Tertiary |
|------|----------------|-----------|----------|
| Race Engineer | Live telemetry traces | Timing + gaps | Strategy predictions |
| Chief Strategist | Gap evolution charts | Pit window timelines | Weather radar |
| Sporting Director | FOM broadcast | Timing tower | Steward communications |
| Performance Engineer | Tire degradation curves | Fuel-corrected pace | ERS deployment |

### 1.4 What Makes Them Iconic

- **Dark backgrounds** with high-contrast traces (noir aesthetic)
- **Dense information** -- 6-12 simultaneous data streams on one display
- **Stacked waveform layout** -- vertically aligned time-series charts sharing an x-axis
- **Multi-colored trace overlays** -- comparing laps or drivers
- **Cursor sync** -- vertical cursor line moves across all stacked traces simultaneously
- **Grid lines** -- subtle, low-contrast for precise reading without visual noise
- **Minimal chrome** -- no decorative elements, every pixel is functional

---

## 2. Steering Wheel Display

### 2.1 Physical Specs

| Property | Value |
|----------|-------|
| Screen size | 4.3 inches |
| Resolution | 480 x 272 pixels |
| Type | Backlit LCD |
| RPM LEDs above screen | 15 LEDs |
| Flag indicator LEDs | Flanking the screen |

### 2.2 Display Pages (switchable via rotary dial)

**Page 1: Race Primary**
```
+---------------------------+
|  DELTA    GEAR    SPEED   |
| -0.234     7      312    |
|                           |
|  LAP 42   FUEL 38.2 kg   |
|  S1: 22.1  S2: 33.4      |
|  ERS: ████░░  62%         |
+---------------------------+
```

**Page 2: Tire Information**
```
+---------------------------+
|     FL: 112C  FR: 108C    |
|     P: 22.1   P: 21.8     |
|                           |
|     RL: 118C  RR: 115C    |
|     P: 19.2   P: 19.0     |
|     WEAR: MED  AGE: 12    |
+---------------------------+
```

**Page 3: ERS/Battery**
```
+---------------------------+
|  DEPLOY MODE: OVERTAKE    |
|  ████████████░░░ 84%      |
|                           |
|  MGU-K: 120 kW            |
|  MGU-H: HARVEST           |
|  LAP ENERGY: 3.2 MJ       |
+---------------------------+
```

### 2.3 RPM LED Bar (above screen)

```
[G][G][G][G][G][Y][Y][Y][R][R][R][R][B][B][B]
 8k  9k  10k 11k 11.5k 12k 12.5k 13k  SHIFT!

G = Green (low RPM range)
Y = Yellow (mid RPM range)
R = Red (approaching shift point)
B = Blue (blinking = SHIFT NOW)
```

### 2.4 Design Principles for Split-Second Reading

- **Large bold numerals** for primary data (gear, delta) -- ~1-2 cm at arm's length
- **High contrast** -- bright on dark, no mid-tones
- **Peripheral cues** -- RPM/flag LEDs handle high-speed decisions without central focus
- **Muscle memory navigation** -- drivers memorize page layouts in simulator
- **Minimal text** -- numbers and symbols over words
- **Color = status** -- green/safe, yellow/warning, red/critical

---

## 3. F1 Broadcast Graphics & Timing Tower

### 3.1 Timing Tower Layout

```
+---+---+---------+--------+---------+------+------+------+-------+------+
|POS|CLR| DRIVER  | TIRE   | GAP     |  S1  |  S2  |  S3  | LAP   |SPEED |
+---+---+---------+--------+---------+------+------+------+-------+------+
| 1 |███| VER     | ●M(12) | LEADER  |22.145|33.456|32.891|1:28.49|  312 |
| 2 |███| NOR     | ●S(8)  | +1.234  |22.301|33.612|32.945|1:28.85|  308 |
| 3 |███| LEC     | ●H(22) | +3.456  |22.567|33.789|33.012|1:29.36|  305 |
| 4 |███| HAM     | ●M(15) | +5.891  |22.678|33.901|33.134|1:29.71|  301 |
+---+---+---------+--------+---------+------+------+------+-------+------+

CLR = Team color bar (2-3px wide vertical stripe)
TIRE = Compound dot (color-coded) + age in laps
GAP = Time to leader (race) or fastest time (qualifying)
```

### 3.2 Color System

#### Sector / Lap Time Colors

| Color | Meaning | Approximate Hex | Usage |
|-------|---------|----------------|-------|
| **Purple** | Session fastest (all-time best) | `#8B00FF` / `#A020F0` | Sector time, lap time |
| **Green** | Personal best | `#00FF00` / `#00E000` | Sector time, lap time |
| **Yellow** | Slower than personal best | `#FFD700` / `#FFFF00` | Sector time, lap time |
| **White** | Most recent / neutral | `#FFFFFF` | Current data, default text |

#### Tire Compound Colors

| Compound | Color | Hex |
|----------|-------|-----|
| Soft | Red | `#FF0000` |
| Medium | Yellow | `#FFFF00` |
| Hard | White | `#FFFFFF` |
| Intermediate | Green | `#00FF00` |
| Wet | Blue | `#0000FF` |

#### Status Colors

| Status | Color | Hex |
|--------|-------|-----|
| Position gained | Green | `#00FF00` |
| Position lost | Red | `#FF0000` |
| In pit lane | White/flashing | -- |
| Retired / DNF | Red strike | `#FF0000` |
| Eliminated (qualifying) | Gray/dimmed | `#666666` |
| DRS available | Green indicator | `#00FF00` |
| Fastest lap (point) | Purple badge | `#8B00FF` |
| Safety Car | Yellow background | `#FFD700` |
| Red Flag | Red background | `#FF0000` |
| VSC | Yellow pulsing | `#FFD700` |

#### Background and Chrome

| Element | Color | Hex |
|---------|-------|-----|
| Primary background | Near-black | `#0A0F1A` |
| Secondary background | Dark navy | `#1A202C` |
| Grid lines | Subtle gray | `#2D3748` |
| Muted text | Dim gray | `#718096` |
| Active text | White | `#FFFFFF` |
| Team color bar | Team-specific | (varies per team) |

### 3.3 Position Change Animation

- **Gained position**: Row slides UP, briefly highlights green
- **Lost position**: Row slides DOWN, briefly highlights red
- **Pit stop**: Row flashes/pulses, shows "PIT" indicator
- **Out lap**: Dimmed row opacity while in pit sequence

---

## 4. ATLAS Telemetry Software (McLaren Applied)

### 4.1 Standard Trace View Layout

The canonical F1 telemetry view stacks traces vertically, all sharing the same x-axis (distance or time):

```
Distance (m) ──────────────────────────────────────>
0         500       1000      1500      2000      2500

SPEED (km/h)  ┌─────────────────────────────────────┐
360 ──────────│     ╱╲      ╱──╲        ╱╲         │
              │   ╱    ╲  ╱      ╲    ╱    ╲       │
180 ──────────│ ╱        ╲          ╲╱        ╲     │
              │╱                                ╲   │
0 ────────────└─────────────────────────────────────┘

THROTTLE (%)  ┌─────────────────────────────────────┐
100 ──────────│████    ████████    ████████    █████ │
              │                                     │
50 ───────────│                                     │
              │                                     │
0 ────────────└─────────────────────────────────────┘

BRAKE (bar)   ┌─────────────────────────────────────┐
100 ──────────│    █           █              █     │
              │   ██          ██             ██     │
50 ───────────│  ███         ███            ███     │
              │ ████        ████           ████     │
0 ────────────└─────────────────────────────────────┘

STEERING (deg)┌─────────────────────────────────────┐
+180 ─────────│         ╱╲                ╱╲        │
0 ────────────│────────╱──╲──────────────╱──╲───────│
-180 ─────────│              ╲╱                ╲╱   │
              └─────────────────────────────────────┘

GEAR          ┌─────────────────────────────────────┐
8 ────────────│  8     8   8   8     8   8     8    │
              │   7         7         7              │
4 ────────────│    5     5     5       5     5       │
              │     3     3     3       3     3      │
1 ────────────└─────────────────────────────────────┘
```

### 4.2 Key ATLAS Features

- **Workbook/Page system**: Multiple pages per workbook, each with a custom layout of displays
- **Trace overlays**: Multiple laps overlaid on same chart (e.g., red = current lap, blue = best lap)
- **Timeline navigation**: Graphical bar showing lap sequence (out lap, timed lap, in lap)
- **Cursor sync**: Vertical cursor moves across ALL stacked displays simultaneously
- **Track map**: Auto-generated with corners in green, straights in yellow (based on lateral g-threshold)
- **Gradient cursors**: Compare values at two different distance points
- **Alarm system**: Visual alerts when parameters exceed thresholds
- **Auto-scroll**: Real-time mode follows live data; manual mode for historical analysis

### 4.3 Trace Overlay Convention (Driver Comparison)

```
SPEED ┌────────────────────────────────────────┐
      │  ╱╲  Driver A (solid line)             │
      │ ╱  ╲    ╱──╲                           │
      │╱    ╲  ╱ ···╲·· Driver B (dashed line) │
      │ ····╲╱·       ╲                        │
      └────────────────────────────────────────┘
      Where traces diverge = where time is gained/lost
```

- Traces are overlaid (not transparent), most recent on top
- Engineers scan for **divergence points**: later braking, higher apex speed, earlier throttle application
- Team-specific colors, but commonly: bright primary for Car 1, secondary for Car 2

---

## 5. Race Strategy Visualization

### 5.1 Pit Window Timeline (Gantt-style)

```
Driver     Laps 1    10    20    30    40    50    60    70
VER        ██████████████████░░░░░░░░░░░░░░░░░░░░░░░░░░░
           [  MEDIUM (18 laps)  ][     HARD (52 laps)    ]
                               ^PIT

NOR        ████████████░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░
           [ SOFT (12) ][  MEDIUM (22)  ][  HARD (36)   ]
                       ^PIT            ^PIT

LEC        ░░░░░░░░░░░░░░░████████████░░░░░░░░░░░░░░░░░░
           [   HARD (25)   ][  MED (20) ][ SOFT (25)    ]
                           ^PIT         ^PIT

Colors: ██ Soft=Red  ██ Medium=Yellow  ██ Hard=White
        ░░ = Predicted/optimal window
```

### 5.2 Gap Evolution Chart

```
Gap (s)
  6 ┤
    │                                    ╱ VER-NOR gap growing
  4 ┤                              ╱────╱
    │                    ╱────────╱
  2 ┤          ╱────────╱
    │   ╱─────╱    PIT ↓ (gap drops due to undercut attempt)
  0 ┤──╱───────────╳─────
    │              ╲───── NOR briefly ahead (undercut works!)
 -2 ┤               ╲
    └────────────────────────────────────────────
    Lap 1    10       20       30       40       50
```

### 5.3 Fuel-Corrected Lap Time Chart

```
Lap Time (s)
 92 ┤ Raw times (dots) vs Fuel-corrected (line)
    │  .                                    .  .
 90 ┤   . .                              .
    │      . .  ── corrected trend ──  .
 88 ┤         . . . . . . . . . . . .
    │                    ↑ tire cliff
 86 ┤ ─ ─ ─ ─ ─ ─ ─ ─ ─ ── best pace reference
    │
 84 ┤
    └──────────────────────────────────────────
    Lap 1    10       20       30       40
```

### 5.4 Undercut/Overcut Analysis

```
Time Loss/Gain
  +3s ┤
      │  ████ = Pit stop time loss
  +2s ┤  ████
      │  ████
  +1s ┤  ████     ░░░░ = Time gained on fresh tires
      │  ████     ░░░░
   0s ┤──████─────░░░░──────────────────────
      │           ░░░░ ░░░░
  -1s ┤           ░░░░ ░░░░ ░░░░
      │                ░░░░ ░░░░
  -2s ┤                     ░░░░ = NET GAIN
      └──────────────────────────────────
      PIT    +1    +2    +3    +4    +5 laps after
```

---

## 6. AWS F1 Insights Visualizations

### 6.1 Catalog of AWS Graphics

| Insight | Visualization Type | Description |
|---------|--------------------|-------------|
| **Tire Performance** | Line chart + bar | Tire grip vs. laps, overlaid with gap to car behind |
| **Pit Stop Strategy** | Timeline + prediction | Optimal window bands on lap axis |
| **Track Dominance** | Track map + heatmap | Circuit split into speed zones, color = who is faster |
| **Undercut Threat** | Gauge + timeline | Real-time probability of undercut succeeding |
| **Battle Forecast** | Bar chart + percentage | Overtake probability based on pace differential |
| **Car Performance Score** | Radar chart | Multi-axis rating: power, aero, tire management, etc. |
| **Driver Skill Comparison** | Bar ranking | Historical driver performance normalized across eras |
| **Braking Performance** | Track overlay | Braking zones colored by driver efficiency |
| **Projected Knockout** | Horizontal bar | Target times vs. current times in qualifying |
| **Alternative Strategy** | Branching timeline | What-if scenarios showing different outcomes |
| **Track Pulse** | Live dashboard | Aggregated stream of battles, fastest sectors, top speeds |

### 6.2 Track Dominance Visualization

```
                    ╭──────────╮
                   ╱  SECTOR 1  ╲
                  ╱   VER +0.12   ╲       Speed Zones:
    ╭────────────╯                 ╰─╮     ██ = Low speed corner
    │                                 │    ░░ = Medium speed
    │  ██ T3: NOR +0.04              │    ── = High speed / straight
    │                                 │
    ╰──╮                         ╭───╯
       │  ░░ T5: VER +0.08      │
       ╰────────╮        ╭──────╯
                │SECTOR 2│
                ╰────────╯
                 LEC +0.03

Color per section = dominant driver's team color
```

---

## 7. SciChart: The GPU Engine Behind F1 Dashboards

### 7.1 Performance Specs

| Capability | Value |
|------------|-------|
| Max data points (Windows/WPF) | 100 billion |
| Real-time points without lag | 100 million |
| Sensors per vehicle supported | 1000+ |
| Rendering engine | Visual Xccelerator (VX) -- GPU |
| Signal fidelity | No downsampling, no dropped frames |
| Precision | 64-bit floating point |
| Platforms | WPF, JavaScript, iOS, Android |

### 7.2 Chart Types Used in F1

- **Line charts**: Multi-series telemetry traces (speed, throttle, brake overlaid)
- **Scatter plots**: Lap time distributions, tire data correlations
- **Heatmaps**: Track temperature maps, brake zone analysis
- **Contour plots**: Wind tunnel pressure distributions
- **3D surface plots**: Aero coefficient maps
- **Polar plots**: Tire performance vs. slip angle
- **Force vs. speed graphs**: Drag/downforce curves
- **Multi-axis dashboards**: Synced charts with linked cursors

### 7.3 Key Rendering Features for TUI Inspiration

- **Synced multi-axis charts**: All charts share cursor position
- **Linked legends**: Toggle traces on/off across all charts
- **GPU annotations/markers**: Highlight specific events (pit stop, overtake)
- **Thousands of series**: Handle grid-wide data simultaneously
- **Multi-screen sync**: Same data, different views
- **Bespoke axes**: Custom scales, inverted axes, logarithmic

---

## 8. Concrete TUI Design Patterns Derived from F1

### 8.1 Color Palette for Nika TUI

```
-- BACKGROUNDS --
Primary BG:     #0A0F1A   (near-black, like pit wall screens)
Secondary BG:   #1A202C   (dark navy panels)
Border:         #2D3748   (subtle grid lines)

-- TEXT --
Active:         #FFFFFF   (white, primary data)
Muted:          #718096   (gray, labels and secondary)
Dim:            #4A5568   (very dim, disabled states)

-- PERFORMANCE SEMANTICS (borrowed from F1 timing) --
Fastest/Best:   #A020F0   (purple -- session best)
Personal Best:  #00E000   (green -- improved)
Neutral/Recent: #FFFFFF   (white -- current)
Slower/Warning: #FFD700   (yellow -- degraded)
Error/Critical: #FF0000   (red -- failure)

-- RESOURCE STATES (borrowed from tire compounds) --
High Intensity: #FF0000   (red -- soft tires = aggressive)
Medium Load:    #FFFF00   (yellow -- medium tires = balanced)
Low/Stable:     #FFFFFF   (white -- hard tires = conservative)

-- STATUS --
Active/Running: #00FF00   (green)
Pending/Queue:  #FFD700   (yellow)
Failed/Error:   #FF0000   (red)
Completed:      #A020F0   (purple -- like fastest lap badge)
```

### 8.2 Sparkline Patterns for Workflow Steps

```
Unicode blocks for mini-charts:
▁▂▃▄▅▆▇█

Step latency sparkline (last 10 runs):
  infer:   ▃▄▅▃▂▄▆▃▄▅  avg: 2.3s

Throughput sparkline:
  fetch:   ▇▆▇▅▄▃▂▁▁▁  declining (yellow)

Error rate sparkline:
  exec:    ▁▁▁▁▁▁▁▁▇▇  spike! (red)
```

### 8.3 Stacked Trace View (ATLAS-inspired)

```
TIME ──────────────────────────────────────────>
00:00    00:05    00:10    00:15    00:20    00:25

TOKENS/s ┌─────────────────────────────────────┐
  500 ────│     ╱╲      ╱──╲        ╱╲         │
          │   ╱    ╲  ╱      ╲    ╱    ╲       │ infer: step
  250 ────│ ╱        ╲          ╲╱        ╲     │
          │╱                                ╲   │
    0 ────└─────────────────────────────────────┘

LATENCY  ┌──────────────────────────────────────┐
 5000ms──│    █           █              █      │
         │   ██          ██             ██      │ fetch: step
 2500ms──│  ███         ███            ███      │
         │ ████        ████           ████      │
    0 ───└──────────────────────────────────────┘

MEMORY   ┌──────────────────────────────────────┐
 100% ───│ ─────────────────╱───────────────────│
         │                 ╱                     │ system
  50% ───│ ───────────────╱                      │
         │               ╱  <- GC event          │
   0% ───└──────────────────────────────────────┘
```

### 8.4 Timing Tower (Workflow Step Leaderboard)

```
+---+------+-----------+--------+---------+--------+---------+
|RNK| STEP | STATUS    | LATEST | BEST    | AVG    | DELTA   |
+---+------+-----------+--------+---------+--------+---------+
| 1 | fetch| ● RUNNING | 234ms  | 198ms   | 245ms  | -11ms   |  (green)
| 2 | infer| ● DONE    | 2.34s  | 1.89s   | 2.45s  | +0.12s  |  (yellow)
| 3 | exec | ● QUEUE   | --     | 0.45s   | 0.52s  | --      |  (dim)
| 4 | infer| ● DONE    | 3.12s  | 2.98s   | 3.20s  | -0.08s  |  (green)
| 5 | fetch| ● ERROR   | FAIL   | 201ms   | 230ms  | --      |  (red)
+---+------+-----------+--------+---------+--------+---------+
  ● = colored dot (team color equivalent = verb color)
  DELTA column uses F1 color coding: green=faster, yellow=slower, purple=best ever
```

### 8.5 Steering Wheel Display (Compact Status Panel)

```
╭──────────────────────────────────╮
│  WORKFLOW     STEP 4/7    02:34  │
│   ████████████░░░░░  57%         │
│                                  │
│  TOKENS: 1,247    COST: $0.034   │
│  ETA: 01:45       ERRORS: 0      │
│  RATE: 487 tok/s  MEM: 234 MB    │
╰──────────────────────────────────╯
```

### 8.6 Strategy Timeline (Gantt-style Workflow View)

```
Step          Time ──────────────────────────>
              0s    5s    10s   15s   20s   25s

fetch:api     ████████░░░░
              [GET /data]

infer:gpt4              ██████████████████░░░░░
                        [generating 500 tokens]

exec:convert                                    ████░░
                                                [ffmpeg]

fetch:upload                                          ████████
                                                      [PUT /result]

████ = completed    ░░░░ = estimated remaining
Color = verb type (fetch=blue, infer=purple, exec=green)
```

### 8.7 Gap Analysis (Step-to-Step Delta)

```
Delta vs. Baseline (previous run)
   Step        Current    Baseline    Delta
   ────        ───────    ────────    ─────
   fetch:api   2.34s      2.12s      +0.22s  ██ (yellow, slower)
   infer:gpt4  4.56s      5.01s      -0.45s  ████ (green, faster)
   exec:conv   0.89s      0.88s      +0.01s  (white, neutral)
   fetch:up    1.23s      1.45s      -0.22s  ██ (green, faster)
   ────────────────────────────────────────────
   TOTAL       9.02s      9.46s      -0.44s  IMPROVED (purple!)
```

---

## 9. Key Design Principles Extracted

### From Pit Wall Screens
1. **Dark background is non-negotiable** -- reduces eye strain, maximizes contrast
2. **Information density over simplicity** -- experts want MORE data, not less
3. **Configurable layouts** -- each user role needs different views
4. **Stacked synchronized charts** share the x-axis (time or distance)
5. **Cursor sync across all panels** -- single source of truth for "where am I?"

### From Steering Wheel Display
6. **Hierarchy through size** -- largest number = most important metric
7. **Color = semantics, not decoration** -- every color means something
8. **Pages over scrolling** -- switch context entirely, don't scroll
9. **Peripheral indicators** -- status LEDs / badges for attention without focus
10. **3-second rule** -- if you can't read it in a glance, redesign it

### From Timing Tower
11. **Vertical leaderboard** -- ranked list is the most natural structure
12. **Delta is king** -- absolute values matter less than change
13. **Color-coded deltas** -- purple/green/yellow/red instantly convey meaning
14. **Position changes animate** -- movement catches the eye
15. **Team color stripe** -- 2px of color provides instant identification

### From Strategy Views
16. **Gantt timelines** for sequential processes
17. **Gap evolution lines** show trends, not just current state
18. **What-if branching** -- show alternative outcomes visually
19. **Rolling averages** smooth noise from individual data points
20. **Fuel-correction equivalent** -- normalize metrics to compare fairly

---

## Sources

1. [Red Bull Racing - Bulls' Guide to the Pit Wall](https://www.redbullracing.com/int-en/projects/bulls-guide-to-the-pit-wall/bulls-guide-to-the-pit-wall-communications)
2. [Formula1.com - Insider's Guide to the Pit Wall](https://www.formula1.com/en/latest/article/the-insiders-guide-to-the-pit-wall.CFwHwjTcLNO5Gf4B3Dy4o)
3. [Catapult Sports - How Data Analysis Transforms F1 Race Performance](https://www.catapult.com/blog/f1-data-analysis-transforming-performance)
4. [Mercedes AMG F1 - How Does an F1 Steering Wheel Work?](https://www.mercedesamgf1.com/news/how-does-an-f1-steering-wheel-work)
5. [Motorsport.com - F1 Steering Wheels: How They Work](https://www.motorsport.com/f1/news/f1-steering-wheels-how-they-work-what-the-buttons-do-and-more/10561142/)
6. [AWS - F1 Insights](https://aws.amazon.com/sports/f1/)
7. [AWS Blog - F1 Revs Up Race Day Broadcasts](https://aws.amazon.com/blogs/media/f1-revs-up-race-day-broadcasts-with-real-time-data-storytelling/)
8. [AWS Blog - Formula 1 Using SageMaker](https://aws.amazon.com/blogs/architecture/formula-1-using-amazon-sagemaker-to-deliver-real-time-insights-to-fans-live/)
9. [SciChart - Strengthens F1 Data Visualization](https://www.scichart.com/scichart-strengthens-f1-data-visualization/)
10. [McLaren F1 Playbook - Timing Color System](https://www.mclaren.com/racing/formula-1/playbook/)
11. [F1 Friend - F1 Graphics Explained](https://www.f1friend.com/blog/f1-graphics-explained)
12. [GitHub - WarmBed/PITWALL](https://github.com/WarmBed/PITWALL)
13. [The Fastest Sector - Evolution of the F1 Steering Wheel](https://thefastestsector.com/2025/06/10/from-leather-to-lcd-the-fascinating-evolution-of-the-f1-steering-wheel/)

## Methodology

- Tools used: Perplexity AI (sonar model) for web search aggregation
- Queries executed: 8 targeted research queries
- Sources analyzed: 20+ across official F1 sites, team sites, AWS documentation, engineering tool vendors, fan communities
- Cross-referenced: Color codes verified across McLaren Playbook, F1 Friend, and OverTake.gg community
- Time period covered: 2020-2026 (current regulations era)

## Confidence Level

**High** for color coding system (purple/green/yellow/white is universally documented and standardized by FOM).
**High** for steering wheel display layout and RPM LED patterns (documented by Mercedes, McLaren, Motorsport.com).
**High** for general telemetry architecture (250+ sensors, 17 CAN buses, ~30MB/lap confirmed by multiple sources).
**Medium** for exact ATLAS interface details (proprietary software; described from public screenshots and fan recreations).
**Medium** for exact hex color values in broadcast graphics (approximate; F1 does not publish a public style guide).
**Low** for proprietary team strategy software internals (heavily guarded competitive advantage).

## Further Research Suggestions

- Scrape the GitHub PITWALL project for concrete code patterns in Rust/Python
- Analyze FastF1 Python library's default visualization themes
- Screenshot analysis of actual F1 TV broadcasts for pixel-accurate color matching
- Research Ratatui (Rust TUI library) charting capabilities for sparklines and traces
- Study how sim racing telemetry tools (MoTeC i2, SimHub) implement stacked traces
- Look at Bloomberg Terminal design patterns (another "dense data TUI" paradigm)
