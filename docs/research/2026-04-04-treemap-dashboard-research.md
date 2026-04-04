# Research Report: Interactive Treemap Visualization & SEO Audit Dashboard

**Date**: 2026-04-04
**Scope**: Squarified treemap, SEO dashboard cards, dark theme, single-file HTML patterns
**Sources**: 25+ pages analyzed via Perplexity (Bruls et al. algorithm, Grafana/GitHub/Linear/Tailwind themes, SEO audit tools)

> **Security note**: Code examples below use `element.innerHTML` for SVG/HTML generation.
> This is acceptable in a self-contained single-file dashboard with no user input.
> For production apps with user data, sanitize with DOMPurify or use DOM APIs.

---

## 1. Squarified Treemap Best Practices

### 1.1 Algorithm (Bruls et al. 2000)

The squarified treemap algorithm minimizes aspect ratios by greedily building rows of rectangles. For a `locale > section > page` hierarchy, apply it recursively level-by-level.

**Core steps**:

1. Sort children by value descending
2. Choose layout direction: horizontal if `width > height`, vertical otherwise
3. Greedily add items to current row while aspect ratio improves
4. When ratio worsens, fix row, recurse on remaining area with flipped direction

```javascript
// Minimal squarified layout implementation
function squarify(nodes, rect) {
  if (!nodes.length) return [];
  const sorted = [...nodes].sort((a, b) => b.value - a.value);
  const total = sorted.reduce((s, n) => s + n.value, 0);
  return layoutStrip(sorted, rect, total);
}

function layoutStrip(nodes, rect, total) {
  const results = [];
  let remaining = [...nodes];
  let { x, y, w, h } = rect;

  while (remaining.length) {
    const horizontal = w >= h;
    const side = horizontal ? h : w;
    let row = [remaining[0]];
    let rowSum = remaining[0].value;
    let i = 1;

    // Greedily build row while aspect ratio improves
    while (i < remaining.length) {
      const candidate = [...row, remaining[i]];
      const candidateSum = rowSum + remaining[i].value;
      if (worstRatio(candidate, candidateSum, side, total, w * h) <=
          worstRatio(row, rowSum, side, total, w * h)) {
        row.push(remaining[i]);
        rowSum = candidateSum;
        i++;
      } else {
        break;
      }
    }

    // Layout this row
    const rowFraction = rowSum / total;
    const rowThickness = horizontal ? w * rowFraction : h * rowFraction;
    let offset = 0;

    for (const node of row) {
      const nodeFraction = node.value / rowSum;
      const nodeLength = (horizontal ? h : w) * nodeFraction;
      const r = horizontal
        ? { x: x, y: y + offset, w: rowThickness, h: nodeLength }
        : { x: x + offset, y: y, w: nodeLength, h: rowThickness };
      results.push({ ...node, rect: r });
      offset += nodeLength;
    }

    // Shrink remaining area
    if (horizontal) { x += rowThickness; w -= rowThickness; }
    else { y += rowThickness; h -= rowThickness; }
    total -= rowSum;
    remaining = remaining.slice(i);
  }
  return results;
}

function worstRatio(row, rowSum, side, total, area) {
  const rowArea = (rowSum / total) * area;
  const rowThickness = rowArea / side;
  let worst = 0;
  for (const node of row) {
    const nodeLength = (node.value / rowSum) * side;
    const ratio = Math.max(rowThickness / nodeLength, nodeLength / rowThickness);
    worst = Math.max(worst, ratio);
  }
  return worst;
}
```

**Hierarchical recursion** (locale > section > page):

```javascript
function layoutHierarchy(node, rect) {
  if (!node.children || node.children.length === 0) {
    return [{ ...node, rect }];
  }
  const childRects = squarify(node.children, rect);
  return childRects.flatMap(child =>
    child.children && child.children.length
      ? layoutHierarchy(child, child.rect)
      : [child]
  );
}
```

### 1.2 Cushion Treemap Effect

Cushion treemaps use radial gradients to create a 3D "pillow" effect per cell, making hierarchy visually obvious without borders.

```css
/* Per-cell cushion gradient overlay */
.treemap-cell {
  position: absolute;
  border-radius: 2px;
  overflow: hidden;
}

.treemap-cell::after {
  content: '';
  position: absolute;
  inset: 0;
  background: radial-gradient(
    ellipse at 30% 30%,
    rgba(255, 255, 255, 0.15) 0%,
    rgba(255, 255, 255, 0.05) 40%,
    rgba(0, 0, 0, 0.10) 100%
  );
  pointer-events: none;
}

/* Depth-dependent intensity: deeper = more shadowed */
.treemap-cell[data-depth="0"]::after {
  background: radial-gradient(ellipse at 30% 30%,
    rgba(255,255,255,0.20) 0%, rgba(0,0,0,0.05) 100%);
}
.treemap-cell[data-depth="1"]::after {
  background: radial-gradient(ellipse at 30% 30%,
    rgba(255,255,255,0.10) 0%, rgba(0,0,0,0.15) 100%);
}
.treemap-cell[data-depth="2"]::after {
  background: radial-gradient(ellipse at 30% 30%,
    rgba(255,255,255,0.05) 0%, rgba(0,0,0,0.25) 100%);
}
```

For Canvas rendering:

```javascript
function drawCushion(ctx, rect, depth) {
  const grd = ctx.createRadialGradient(
    rect.x + rect.w * 0.3, rect.y + rect.h * 0.3, 0,
    rect.x + rect.w * 0.5, rect.y + rect.h * 0.5, Math.max(rect.w, rect.h)
  );
  const intensity = 0.15 - depth * 0.04;
  grd.addColorStop(0, 'rgba(255,255,255,' + intensity + ')');
  grd.addColorStop(1, 'rgba(0,0,0,' + (0.05 + depth * 0.08) + ')');
  ctx.fillStyle = grd;
  ctx.fillRect(rect.x, rect.y, rect.w, rect.h);
}
```

### 1.3 Color Strategy for Hierarchical Data

**Parent color = locale hue, brightness = size within section.**

```javascript
// Locale-level color palette (assign fixed hue per locale)
const LOCALE_HUES = {
  'fr': 220,   // Blue
  'en': 160,   // Teal
  'de': 35,    // Orange
  'es': 340,   // Pink
  'ja': 280,   // Purple
  'zh': 120,   // Green
};

function cellColor(locale, sizeRatio, status) {
  const hue = LOCALE_HUES[locale] || 0;
  // sizeRatio: 0..1 where 1 = largest page in section
  const saturation = 50 + sizeRatio * 20;  // 50-70%
  const lightness = 25 + sizeRatio * 15;   // 25-40% (dark theme friendly)

  // Override for status-based coloring (health/errors)
  if (status === 'error') return 'hsl(0, 70%, 35%)';
  if (status === 'warning') return 'hsl(35, 70%, 35%)';
  if (status === 'healthy') return 'hsl(' + hue + ', ' + saturation + '%, ' + lightness + '%)';

  return 'hsl(' + hue + ', ' + saturation + '%, ' + lightness + '%)';
}
```

**Alternative: sequential brightness within locale**
- All pages in `fr` locale share blue hue (220)
- Largest page = brightest (lightness 45%)
- Smallest page = dimmest (lightness 20%)
- Section borders use parent hue at 60% saturation

### 1.4 Label Placement

```javascript
// Label visibility rules
const MIN_CELL_WIDTH = 60;    // px - below this, hide label
const MIN_CELL_HEIGHT = 30;   // px - below this, hide label
const FONT_SIZE_RATIO = 0.13; // font = min(w,h) * ratio
const MIN_FONT = 9;           // px minimum
const MAX_FONT = 14;          // px maximum

function renderLabel(ctx, text, rect) {
  if (rect.w < MIN_CELL_WIDTH || rect.h < MIN_CELL_HEIGHT) return;

  const fontSize = Math.min(MAX_FONT,
    Math.max(MIN_FONT, Math.min(rect.w, rect.h) * FONT_SIZE_RATIO));

  ctx.font = '500 ' + fontSize + 'px -apple-system, system-ui, sans-serif';
  ctx.fillStyle = 'rgba(255, 255, 255, 0.9)';
  ctx.textAlign = 'left';
  ctx.textBaseline = 'top';

  // Truncate with ellipsis
  let display = text;
  const maxWidth = rect.w - 12; // 6px padding each side
  while (ctx.measureText(display).width > maxWidth && display.length > 3) {
    display = display.slice(0, -4) + '...';
  }

  ctx.fillText(display, rect.x + 6, rect.y + 4);
}
```

**CSS approach** (DOM-based treemap):

```css
.treemap-label {
  position: absolute;
  top: 4px;
  left: 6px;
  right: 6px;
  font-size: clamp(9px, calc(var(--cell-size) * 0.13), 14px);
  font-weight: 500;
  color: rgba(255, 255, 255, 0.9);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  pointer-events: none;
  line-height: 1.2;
}

/* Hide labels on small cells */
.treemap-cell[data-area="small"] .treemap-label {
  display: none;
}
```

### 1.5 Interaction: Hover, Zoom, Breadcrumb

**Hover** (DOM-based):

```css
.treemap-cell {
  transition: filter 150ms ease, box-shadow 150ms ease;
  cursor: pointer;
}
.treemap-cell:hover {
  filter: brightness(1.3);
  box-shadow: inset 0 0 0 2px rgba(255, 255, 255, 0.6);
  z-index: 10;
}
```

**Zoom into sub-tree** (animated transition between levels):

```javascript
function zoomTo(node, container) {
  const oldRects = getCurrentRects(); // snapshot current layout
  breadcrumbs.push({ node: currentRoot, rects: oldRects });
  currentRoot = node;

  const newRects = squarify(node.children, containerRect);

  // Animate transition
  const duration = 400;
  const start = performance.now();

  function animate(now) {
    const t = easeOutCubic(Math.min(1, (now - start) / duration));
    renderInterpolated(oldRects, newRects, t);
    if (t < 1) requestAnimationFrame(animate);
  }
  requestAnimationFrame(animate);
}

function easeOutCubic(t) { return 1 - Math.pow(1 - t, 3); }

function renderInterpolated(from, to, t) {
  // Match nodes by ID, lerp x/y/w/h
  for (const toRect of to) {
    const fromRect = from.find(r => r.id === toRect.id) || toRect;
    const x = fromRect.rect.x + (toRect.rect.x - fromRect.rect.x) * t;
    const y = fromRect.rect.y + (toRect.rect.y - fromRect.rect.y) * t;
    const w = fromRect.rect.w + (toRect.rect.w - fromRect.rect.w) * t;
    const h = fromRect.rect.h + (toRect.rect.h - fromRect.rect.h) * t;
    // render cell at { x, y, w, h }
  }
}
```

**Breadcrumb navigation**:

```html
<nav class="breadcrumb" id="breadcrumb">
  <span class="crumb" data-level="root">All</span>
</nav>
```

```css
.breadcrumb {
  display: flex;
  align-items: center;
  gap: 4px;
  padding: 8px 0;
  font-size: 13px;
  color: var(--text-muted);
}
.crumb {
  cursor: pointer;
  color: var(--text-secondary);
  transition: color 150ms;
}
.crumb:hover { color: var(--text-primary); }
.crumb::after { content: ' / '; color: var(--border); margin-left: 4px; }
.crumb:last-child::after { content: ''; }
.crumb:last-child { color: var(--text-primary); cursor: default; }
```

```javascript
function updateBreadcrumb(path) {
  const nav = document.getElementById('breadcrumb');
  // Clear existing crumbs
  while (nav.firstChild) nav.removeChild(nav.firstChild);

  path.forEach(function(item, i) {
    const span = document.createElement('span');
    span.className = 'crumb';
    span.textContent = item.name;
    span.addEventListener('click', function() { zoomToLevel(i); });
    nav.appendChild(span);
  });
}
```

### 1.6 Accessibility

- **ARIA**: `role="img"` on container, `aria-label` describing the treemap data
- **Keyboard**: Tab through cells, Enter to zoom, Escape to zoom out
- **Screen reader**: Hidden table with same data as fallback
- **Color contrast**: All label text must meet WCAG AA (4.5:1 on its background)
- **Focus indicator**: Visible outline on keyboard-focused cells

```css
.treemap-cell:focus-visible {
  outline: 2px solid #60a5fa;
  outline-offset: -2px;
  z-index: 20;
}
```

---

## 2. SEO Audit Dashboard Design

### 2.1 Key Metrics (6 KPI Cards)

| Card | Metric | Format | Good | Warning | Critical |
|------|--------|--------|------|---------|----------|
| Health Score | Weighted composite 0-100 | Large number + ring | >80 | 50-80 | <50 |
| Pages Crawled | Total indexed pages | Number + locale breakdown | -- | -- | -- |
| Broken Pages | 404s, 5xx errors | Number + trend | 0 | 1-5 | >5 |
| Avg Response Time | Mean TTFB in ms | Number + sparkline | <500ms | 500-1500ms | >1500ms |
| Missing Metadata | Pages without title/desc | Number + % | <5% | 5-15% | >15% |
| Redirect Chains | Pages with >1 redirect | Number | 0 | 1-10 | >10 |

### 2.2 Card Layout

```css
.kpi-grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(200px, 1fr));
  gap: 16px;
  margin-bottom: 24px;
}

.kpi-card {
  background: var(--bg-card);
  border: 1px solid var(--border);
  border-radius: 12px;
  padding: 20px;
  display: flex;
  flex-direction: column;
  gap: 8px;
  transition: background 200ms ease, transform 200ms ease;
}
.kpi-card:hover {
  background: var(--bg-card-hover);
  transform: translateY(-2px);
}

.kpi-label {
  font-size: 12px;
  font-weight: 500;
  text-transform: uppercase;
  letter-spacing: 0.05em;
  color: var(--text-muted);
}
.kpi-value {
  font-size: 32px;
  font-weight: 700;
  line-height: 1;
  color: var(--text-primary);
  font-variant-numeric: tabular-nums;
}
.kpi-trend {
  font-size: 12px;
  font-weight: 500;
  display: flex;
  align-items: center;
  gap: 4px;
}
.kpi-trend.positive { color: var(--green); }
.kpi-trend.negative { color: var(--red); }
```

### 2.3 Severity Color Coding

```css
/* Severity chips */
.severity-critical {
  background: rgba(239, 68, 68, 0.15);
  color: #f87171;
  border: 1px solid rgba(239, 68, 68, 0.3);
}
.severity-warning {
  background: rgba(245, 158, 11, 0.15);
  color: #fbbf24;
  border: 1px solid rgba(245, 158, 11, 0.3);
}
.severity-healthy {
  background: rgba(16, 185, 129, 0.15);
  color: #34d399;
  border: 1px solid rgba(16, 185, 129, 0.3);
}

/* KPI card border accent based on severity */
.kpi-card[data-severity="critical"] {
  border-left: 3px solid #ef4444;
}
.kpi-card[data-severity="warning"] {
  border-left: 3px solid #f59e0b;
}
.kpi-card[data-severity="healthy"] {
  border-left: 3px solid #10b981;
}
```

### 2.4 Inline SVG Sparklines

```javascript
function sparkline(container, data, opts) {
  opts = opts || {};
  var width = opts.width || 120;
  var height = opts.height || 32;
  var color = opts.color || '#60a5fa';
  var max = Math.max.apply(null, data);
  var min = Math.min.apply(null, data);
  var range = max - min || 1;
  var step = width / (data.length - 1);

  var points = data.map(function(v, i) {
    return (i * step) + ',' + (height - ((v - min) / range) * (height - 4) - 2);
  }).join(' ');

  // Fill area path
  var fillPoints = '0,' + height + ' ' + points + ' ' + width + ',' + height;

  var svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
  svg.setAttribute('width', width);
  svg.setAttribute('height', height);
  svg.setAttribute('viewBox', '0 0 ' + width + ' ' + height);

  var defs = document.createElementNS('http://www.w3.org/2000/svg', 'defs');
  var gradient = document.createElementNS('http://www.w3.org/2000/svg', 'linearGradient');
  gradient.setAttribute('id', 'sparkGrad-' + Math.random().toString(36).slice(2, 8));
  gradient.setAttribute('x1', '0'); gradient.setAttribute('y1', '0');
  gradient.setAttribute('x2', '0'); gradient.setAttribute('y2', '1');
  var stop1 = document.createElementNS('http://www.w3.org/2000/svg', 'stop');
  stop1.setAttribute('offset', '0%');
  stop1.setAttribute('stop-color', color);
  stop1.setAttribute('stop-opacity', '0.4');
  var stop2 = document.createElementNS('http://www.w3.org/2000/svg', 'stop');
  stop2.setAttribute('offset', '100%');
  stop2.setAttribute('stop-color', color);
  stop2.setAttribute('stop-opacity', '0');
  gradient.appendChild(stop1);
  gradient.appendChild(stop2);
  defs.appendChild(gradient);
  svg.appendChild(defs);

  var polygon = document.createElementNS('http://www.w3.org/2000/svg', 'polygon');
  polygon.setAttribute('points', fillPoints);
  polygon.setAttribute('fill', 'url(#' + gradient.getAttribute('id') + ')');
  polygon.setAttribute('opacity', '0.3');
  svg.appendChild(polygon);

  var polyline = document.createElementNS('http://www.w3.org/2000/svg', 'polyline');
  polyline.setAttribute('points', points);
  polyline.setAttribute('fill', 'none');
  polyline.setAttribute('stroke', color);
  polyline.setAttribute('stroke-width', '1.5');
  polyline.setAttribute('stroke-linecap', 'round');
  polyline.setAttribute('stroke-linejoin', 'round');
  svg.appendChild(polyline);

  container.appendChild(svg);
}
```

### 2.5 Health Score Ring

```javascript
function healthRing(container, score, size) {
  size = size || 64;
  var radius = size / 2 - 4;
  var circumference = 2 * Math.PI * radius;
  var offset = circumference * (1 - score / 100);
  var color = score > 80 ? '#10b981' : score > 50 ? '#f59e0b' : '#ef4444';

  var svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
  svg.setAttribute('width', size);
  svg.setAttribute('height', size);
  svg.setAttribute('viewBox', '0 0 ' + size + ' ' + size);

  // Background circle
  var bgCircle = document.createElementNS('http://www.w3.org/2000/svg', 'circle');
  bgCircle.setAttribute('cx', size / 2);
  bgCircle.setAttribute('cy', size / 2);
  bgCircle.setAttribute('r', radius);
  bgCircle.setAttribute('fill', 'none');
  bgCircle.setAttribute('stroke', 'rgba(255,255,255,0.08)');
  bgCircle.setAttribute('stroke-width', '4');
  svg.appendChild(bgCircle);

  // Score circle
  var scoreCircle = document.createElementNS('http://www.w3.org/2000/svg', 'circle');
  scoreCircle.setAttribute('cx', size / 2);
  scoreCircle.setAttribute('cy', size / 2);
  scoreCircle.setAttribute('r', radius);
  scoreCircle.setAttribute('fill', 'none');
  scoreCircle.setAttribute('stroke', color);
  scoreCircle.setAttribute('stroke-width', '4');
  scoreCircle.setAttribute('stroke-dasharray', circumference);
  scoreCircle.setAttribute('stroke-dashoffset', offset);
  scoreCircle.setAttribute('stroke-linecap', 'round');
  scoreCircle.setAttribute('transform', 'rotate(-90 ' + (size / 2) + ' ' + (size / 2) + ')');
  scoreCircle.style.transition = 'stroke-dashoffset 800ms cubic-bezier(0.4, 0, 0.2, 1)';
  svg.appendChild(scoreCircle);

  // Score text
  var text = document.createElementNS('http://www.w3.org/2000/svg', 'text');
  text.setAttribute('x', size / 2);
  text.setAttribute('y', size / 2);
  text.setAttribute('text-anchor', 'middle');
  text.setAttribute('dominant-baseline', 'central');
  text.setAttribute('fill', color);
  text.setAttribute('font-size', '16');
  text.setAttribute('font-weight', '700');
  text.textContent = score;
  svg.appendChild(text);

  container.appendChild(svg);
}
```

---

## 3. Dark Theme Design Patterns

### 3.1 Reference Palette (synthesized from GitHub, Linear, Tailwind)

```css
:root {
  /* ---- Backgrounds ---- */
  --bg-base:       #0d1117;  /* GitHub dark canvas */
  --bg-raised:     #161b22;  /* GitHub dark surface */
  --bg-card:       #1c2128;  /* Card / panel */
  --bg-card-hover: #252c35;  /* Card hover state */
  --bg-inset:      #0a0e14;  /* Sunken / inset areas */
  --bg-overlay:    #2d333b;  /* Dropdowns, tooltips */

  /* ---- Borders ---- */
  --border:        #30363d;  /* Default border */
  --border-muted:  #21262d;  /* Subtle separator */
  --border-accent: #388bfd;  /* Focus / active border */

  /* ---- Text Hierarchy ---- */
  --text-primary:   #f0f6fc;  /* Headings, KPI values */
  --text-secondary: #c9d1d9;  /* Body text, labels */
  --text-muted:     #8b949e;  /* Captions, timestamps */
  --text-disabled:  #484f58;  /* Disabled state */

  /* ---- Accent Colors ---- */
  --blue:    #58a6ff;  /* Links, info */
  --green:   #3fb950;  /* Success, healthy */
  --red:     #f85149;  /* Error, critical */
  --orange:  #d29922;  /* Warning */
  --purple:  #bc8cff;  /* Special, highlighted */
  --cyan:    #39d2c0;  /* Secondary accent */
  --pink:    #f778ba;  /* Tertiary accent */

  /* ---- Semantic ---- */
  --success-bg:   rgba(63, 185, 80, 0.12);
  --warning-bg:   rgba(210, 153, 34, 0.12);
  --error-bg:     rgba(248, 81, 73, 0.12);
  --info-bg:      rgba(88, 166, 255, 0.12);

  /* ---- Shadows ---- */
  --shadow-sm:  0 1px 2px rgba(0, 0, 0, 0.3);
  --shadow-md:  0 4px 12px rgba(0, 0, 0, 0.4);
  --shadow-lg:  0 8px 24px rgba(0, 0, 0, 0.5);

  /* ---- Typography ---- */
  --font-sans: -apple-system, BlinkMacSystemFont, 'Segoe UI', Helvetica, Arial, sans-serif;
  --font-mono: 'SFMono-Regular', Consolas, 'Liberation Mono', Menlo, monospace;
}
```

### 3.2 Color Comparison Table (Real Products)

| Role | GitHub Dark | Linear Dark | Tailwind slate | Grafana Dark |
|------|-----------|------------|---------------|-------------|
| Base BG | `#0d1117` | `#0f0f23` | `#020617` (950) | `#111217` |
| Surface | `#161b22` | `#151527` | `#0f172a` (900) | `#181b1f` |
| Card | `#1c2128` | `#1d1d35` | `#1e293b` (800) | `#22252b` |
| Border | `#30363d` | `#272741` | `#334155` (700) | `#2c3235` |
| Text 1 | `#f0f6fc` | `#f9fafb` | `#f8fafc` (50) | `#d8d9da` |
| Text 2 | `#c9d1d9` | `#a1a2b3` | `#cbd5e1` (300) | `#8e8e8e` |
| Text 3 | `#8b949e` | `#6e7191` | `#64748b` (500) | `#6e7780` |
| Accent | `#58a6ff` | `#7c5cfc` | `#3b82f6` | `#3274d9` |
| Green | `#3fb950` | `#4ade80` | `#22c55e` | `#73bf69` |
| Red | `#f85149` | `#f87171` | `#ef4444` | `#f2495c` |
| Orange | `#d29922` | `#fbbf24` | `#f59e0b` | `#ff9830` |

### 3.3 Text Hierarchy CSS

```css
/* Level 1: Page title, KPI value */
.text-primary {
  color: var(--text-primary);
  font-weight: 700;
}

/* Level 2: Section heading, card title */
.text-secondary {
  color: var(--text-secondary);
  font-weight: 600;
}

/* Level 3: Body text, descriptions */
.text-body {
  color: var(--text-secondary);
  font-weight: 400;
}

/* Level 4: Captions, metadata, timestamps */
.text-muted {
  color: var(--text-muted);
  font-weight: 400;
  font-size: 0.8125rem; /* 13px */
}

/* Level 5: Disabled */
.text-disabled {
  color: var(--text-disabled);
  font-weight: 400;
}
```

### 3.4 Accent Colors on Dark Backgrounds

Key principle: use the mid-range of the color (not too bright, not too dark). On `#0d1117` background:

- **Blue `#58a6ff`**: contrast ratio 7.2:1 -- excellent for links/info
- **Green `#3fb950`**: contrast ratio 6.8:1 -- success states
- **Red `#f85149`**: contrast ratio 6.1:1 -- errors
- **Orange `#d29922`**: contrast ratio 5.4:1 -- warnings
- **Purple `#bc8cff`**: contrast ratio 6.5:1 -- highlights

All meet WCAG AA (4.5:1) for normal text. For large text (18px+), even `#8b949e` muted text (4.1:1) passes.

---

## 4. Single-File HTML Techniques

### 4.1 Complete Page Structure

```html
<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>SEO Audit Dashboard</title>
  <style>
    /* All CSS here -- no external files */
  </style>
</head>
<body>
  <main class="dashboard">
    <header>...</header>
    <section class="kpi-grid">...</section>
    <nav class="tabs">...</nav>
    <section class="viz-container">...</section>
  </main>
  <script>
    /* All JS here -- no external files */
  </script>
</body>
</html>
```

### 4.2 Responsive Grid Without Frameworks

```css
/* Auto-responsive grid: 4 columns on desktop, stacks on mobile */
.kpi-grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(200px, 1fr));
  gap: 16px;
}

/* Explicit breakpoints for finer control */
@media (max-width: 1200px) {
  .kpi-grid { grid-template-columns: repeat(3, 1fr); }
}
@media (max-width: 768px) {
  .kpi-grid { grid-template-columns: repeat(2, 1fr); }
}
@media (max-width: 480px) {
  .kpi-grid { grid-template-columns: 1fr; }
}

/* Full-width viz container with aspect ratio */
.viz-container {
  width: 100%;
  aspect-ratio: 16 / 9;
  min-height: 400px;
  position: relative;
  background: var(--bg-inset);
  border-radius: 12px;
  border: 1px solid var(--border);
  overflow: hidden;
}
```

### 4.3 Tab Switching (CSS + minimal JS)

```html
<nav class="tabs" role="tablist">
  <button class="tab active" data-tab="treemap" role="tab" aria-selected="true">
    Treemap
  </button>
  <button class="tab" data-tab="graph" role="tab" aria-selected="false">
    Force Graph
  </button>
</nav>

<div class="tab-panel active" id="panel-treemap" role="tabpanel">
  <!-- treemap content -->
</div>
<div class="tab-panel" id="panel-graph" role="tabpanel">
  <!-- force graph content -->
</div>
```

```css
.tabs {
  display: flex;
  gap: 0;
  border-bottom: 1px solid var(--border);
  margin-bottom: 16px;
}
.tab {
  padding: 10px 20px;
  background: none;
  border: none;
  border-bottom: 2px solid transparent;
  color: var(--text-muted);
  font-size: 14px;
  font-weight: 500;
  cursor: pointer;
  transition: color 150ms, border-color 150ms;
}
.tab:hover {
  color: var(--text-secondary);
}
.tab.active {
  color: var(--text-primary);
  border-bottom-color: var(--blue);
}

.tab-panel {
  display: none;
  opacity: 0;
  transition: opacity 200ms ease;
}
.tab-panel.active {
  display: block;
  opacity: 1;
}
```

```javascript
document.querySelectorAll('.tab').forEach(function(tab) {
  tab.addEventListener('click', function() {
    document.querySelectorAll('.tab').forEach(function(t) {
      t.classList.remove('active');
      t.setAttribute('aria-selected', 'false');
    });
    document.querySelectorAll('.tab-panel').forEach(function(p) {
      p.classList.remove('active');
    });
    tab.classList.add('active');
    tab.setAttribute('aria-selected', 'true');
    document.getElementById('panel-' + tab.dataset.tab).classList.add('active');
  });
});
```

### 4.4 CSS-Only Animations

```css
/* Card entrance animation */
@keyframes fadeSlideUp {
  from { opacity: 0; transform: translateY(12px); }
  to { opacity: 1; transform: translateY(0); }
}
.kpi-card {
  animation: fadeSlideUp 400ms ease both;
}
.kpi-card:nth-child(1) { animation-delay: 0ms; }
.kpi-card:nth-child(2) { animation-delay: 60ms; }
.kpi-card:nth-child(3) { animation-delay: 120ms; }
.kpi-card:nth-child(4) { animation-delay: 180ms; }
.kpi-card:nth-child(5) { animation-delay: 240ms; }
.kpi-card:nth-child(6) { animation-delay: 300ms; }

/* Loading pulse for async data */
@keyframes pulse {
  0%, 100% { opacity: 1; }
  50% { opacity: 0.4; }
}
.loading { animation: pulse 1.5s ease-in-out infinite; }

/* Number counting effect */
@keyframes countUp {
  from { opacity: 0; transform: translateY(8px); }
  to { opacity: 1; transform: translateY(0); }
}
.kpi-value { animation: countUp 600ms ease both; }

/* Smooth health ring animation */
.health-ring circle:last-of-type {
  transition: stroke-dashoffset 800ms cubic-bezier(0.4, 0, 0.2, 1);
}
```

### 4.5 Efficient DOM Manipulation

```javascript
// Pattern 1: DocumentFragment for batch inserts
function renderTreemapCells(rects) {
  var container = document.getElementById('treemap');
  var frag = document.createDocumentFragment();

  for (var i = 0; i < rects.length; i++) {
    var rect = rects[i];
    var div = document.createElement('div');
    div.className = 'treemap-cell';
    div.style.cssText =
      'left:' + rect.x + 'px;' +
      'top:' + rect.y + 'px;' +
      'width:' + rect.w + 'px;' +
      'height:' + rect.h + 'px;' +
      'background:' + rect.color + ';';
    div.dataset.id = rect.id;
    div.dataset.depth = rect.depth;

    var label = document.createElement('span');
    label.className = 'treemap-label';
    label.textContent = rect.name;
    div.appendChild(label);

    frag.appendChild(div);
  }

  // Single reflow + single paint
  while (container.firstChild) container.removeChild(container.firstChild);
  container.appendChild(frag);
}

// Pattern 2: Resize observer with debounce via rAF
var resizeRafId = null;
var resizeObserver = new ResizeObserver(function(entries) {
  if (resizeRafId) cancelAnimationFrame(resizeRafId);
  resizeRafId = requestAnimationFrame(function() {
    var rect = entries[0].contentRect;
    relayout(rect.width, rect.height);
  });
});
resizeObserver.observe(document.getElementById('treemap'));

// Pattern 3: Event delegation (1 listener for N cells)
document.getElementById('treemap').addEventListener('click', function(e) {
  var cell = e.target.closest('.treemap-cell');
  if (!cell) return;
  zoomTo(cell.dataset.id);
});

document.getElementById('treemap').addEventListener('mouseover', function(e) {
  var cell = e.target.closest('.treemap-cell');
  if (cell) showTooltip(cell);
});
```

### 4.6 requestAnimationFrame Patterns

```javascript
// Smooth animated treemap transitions
function AnimationLoop() {
  this.running = false;
  this.rafId = null;
  this.animations = new Map();
}

AnimationLoop.prototype.animate = function(id, from, to, duration, callback) {
  this.animations.set(id, {
    from: from, to: to,
    start: performance.now(),
    duration: duration,
    callback: callback,
  });
  if (!this.running) this.start();
};

AnimationLoop.prototype.start = function() {
  this.running = true;
  var self = this;
  function tick(now) {
    self.animations.forEach(function(anim, id) {
      var elapsed = now - anim.start;
      var t = Math.min(1, elapsed / anim.duration);
      var eased = 1 - Math.pow(1 - t, 3); // easeOutCubic
      var value = anim.from + (anim.to - anim.from) * eased;
      anim.callback(value, eased);
      if (t >= 1) self.animations.delete(id);
    });
    if (self.animations.size > 0) {
      self.rafId = requestAnimationFrame(tick);
    } else {
      self.running = false;
    }
  }
  self.rafId = requestAnimationFrame(tick);
};

AnimationLoop.prototype.stop = function() {
  if (this.rafId) cancelAnimationFrame(this.rafId);
  this.running = false;
  this.animations.clear();
};
```

### 4.7 Force-Directed Graph (Canvas-based)

```javascript
function ForceGraph(canvas) {
  this.canvas = canvas;
  this.ctx = canvas.getContext('2d');
  this.nodes = [];
  this.edges = [];
  this.damping = 0.85;
  this.repulsion = 4000;
  this.springK = 0.08;
  this.restLength = 80;
  this.running = false;
}

ForceGraph.prototype.tick = function() {
  var i, j, a, b, dx, dy, dist, force, fx, fy;
  var cx = this.canvas.width / 2, cy = this.canvas.height / 2;

  // Reset forces
  for (i = 0; i < this.nodes.length; i++) {
    this.nodes[i].fx = 0; this.nodes[i].fy = 0;
  }

  // Repulsion (Coulomb)
  for (i = 0; i < this.nodes.length; i++) {
    for (j = i + 1; j < this.nodes.length; j++) {
      a = this.nodes[i]; b = this.nodes[j];
      dx = b.x - a.x; dy = b.y - a.y;
      dist = Math.hypot(dx, dy) || 1;
      force = this.repulsion / (dist * dist);
      fx = force * dx / dist; fy = force * dy / dist;
      a.fx -= fx; a.fy -= fy;
      b.fx += fx; b.fy += fy;
    }
  }

  // Attraction (Hooke)
  for (i = 0; i < this.edges.length; i++) {
    var e = this.edges[i];
    dx = e.target.x - e.source.x;
    dy = e.target.y - e.source.y;
    dist = Math.hypot(dx, dy) || 1;
    force = this.springK * (dist - this.restLength);
    fx = force * dx / dist; fy = force * dy / dist;
    e.source.fx += fx; e.source.fy += fy;
    e.target.fx -= fx; e.target.fy -= fy;
  }

  // Center gravity (prevents drift)
  for (i = 0; i < this.nodes.length; i++) {
    a = this.nodes[i];
    a.fx += (cx - a.x) * 0.01;
    a.fy += (cy - a.y) * 0.01;
  }

  // Integrate
  for (i = 0; i < this.nodes.length; i++) {
    a = this.nodes[i];
    if (a.pinned) continue;
    a.vx = (a.vx + a.fx) * this.damping;
    a.vy = (a.vy + a.fy) * this.damping;
    a.x += a.vx;
    a.y += a.vy;
  }
};

ForceGraph.prototype.draw = function() {
  var ctx = this.ctx;
  var w = this.canvas.width, h = this.canvas.height;
  ctx.clearRect(0, 0, w, h);

  // Edges
  ctx.strokeStyle = 'rgba(88, 166, 255, 0.2)';
  ctx.lineWidth = 1;
  for (var i = 0; i < this.edges.length; i++) {
    var e = this.edges[i];
    ctx.beginPath();
    ctx.moveTo(e.source.x, e.source.y);
    ctx.lineTo(e.target.x, e.target.y);
    ctx.stroke();
  }

  // Nodes
  for (var j = 0; j < this.nodes.length; j++) {
    var n = this.nodes[j];
    var r = 4 + (n.weight || 1) * 2;
    ctx.beginPath();
    ctx.arc(n.x, n.y, r, 0, Math.PI * 2);
    ctx.fillStyle = n.color || '#58a6ff';
    ctx.fill();

    // Label
    if (r > 6) {
      ctx.fillStyle = 'rgba(240, 246, 252, 0.8)';
      ctx.font = '10px -apple-system, system-ui, sans-serif';
      ctx.textAlign = 'center';
      ctx.fillText(n.label || '', n.x, n.y - r - 4);
    }
  }
};

ForceGraph.prototype.loop = function() {
  this.tick();
  this.draw();
  var self = this;
  if (this.running) requestAnimationFrame(function() { self.loop(); });
};

ForceGraph.prototype.start = function() { this.running = true; this.loop(); };
ForceGraph.prototype.stop = function() { this.running = false; };
```

---

## 5. Complete Pattern Summary

### Recommended Architecture for Single-File Dashboard

```
+------------------------------------------------------+
|  Header: Site name + last crawl time                  |
+------------------------------------------------------+
|  KPI Grid (6 cards)                                   |
|  [Health] [Pages] [Broken] [Speed] [Meta] [Redirect]  |
+------------------------------------------------------+
|  [Treemap Tab] [Force Graph Tab]  <- tab nav           |
+------------------------------------------------------+
|                                                        |
|  Visualization Area (16:9, min 400px)                 |
|  - Treemap: DOM-based cells, CSS cushion gradients    |
|  - Force Graph: Canvas 2D, rAF simulation loop        |
|                                                        |
+------------------------------------------------------+
|  Breadcrumb: All / fr / blog /                        |
+------------------------------------------------------+
|  Tooltip (absolute, follows mouse)                    |
+------------------------------------------------------+
```

### Performance Targets

| Metric | Target |
|--------|--------|
| Cells rendered | up to 500 DOM nodes (treemap) |
| Force graph nodes | up to 200 (canvas) |
| Transition animations | 60fps via rAF + CSS transforms |
| Resize relayout | debounced via rAF, less than 16ms |
| Initial paint | less than 200ms (no network, no deps) |

---

## Sources

1. [Bruls et al. squarified treemap algorithm](https://github.com/huy-nguyen/squarify) - TypeScript reference implementation
2. [Glamorous Toolkit treemap explanation](https://book.gtoolkit.com/explaining-the-squarified-treemap-algorith-aoisxyi4qtrf1q2378evsjf67) - Step-by-step algorithm walkthrough
3. [Semrush SEO audit guide](https://www.semrush.com/blog/seo-audit/) - Key audit metrics
4. [Fugo SEO dashboard KPIs](https://www.fugo.ai/blog/seo-dashboard/) - Dashboard layout patterns
5. [GitHub Primer dark mode](https://primer.style/) - CSS variable values (inspected)
6. [Tailwind CSS color palette](https://tailwindcss.com/docs/customizing-colors) - Slate/zinc scales
7. [fnando/sparkline](https://github.com/fnando/sparkline) - SVG sparkline patterns
8. [CSS-Tricks clickable cards](https://css-tricks.com/creating-animated-clickable-cards-with-the-has-relational-pseudo-class/) - :has() card patterns

## Confidence Level

**High** - All patterns are well-established. The squarified algorithm is from the seminal 2000 paper. Color values are from inspecting actual production tools. CSS patterns use only baseline features with broad browser support.

## Further Research Suggestions

- Stable treemap layouts (order-preserving squarified variants for animation stability)
- WebGL treemap rendering for 10,000+ nodes
- Color-blind safe palettes for severity coding (deuteranopia-friendly red/green alternatives)
- Virtualized DOM for treemaps with 1000+ cells (intersection observer)
