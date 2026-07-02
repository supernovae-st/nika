/* scene.js — deterministic timeline engine for Nika motion scenes.
 *
 * A scene declares meta + steps. `window.__seek(ms)` applies the COMPLETE
 * visual state for time t — pure function of t, so a headless renderer can
 * step frame by frame and a browser preview can free-run the same file.
 *
 * Easing follows the Nika motion language: structure emerges — align,
 * draw, localize, materialize. No wobble, no particles.
 */
(function () {
  "use strict";

  // cubic-bezier(x1,y1,x2,y2) solved for y given time-fraction x
  function bezier(x1, y1, x2, y2) {
    const cx = (t) => 3 * x1 * t * (1 - t) ** 2 + 3 * x2 * t * t * (1 - t) + t ** 3;
    const cy = (t) => 3 * y1 * t * (1 - t) ** 2 + 3 * y2 * t * t * (1 - t) + t ** 3;
    return (x) => {
      if (x <= 0) return 0;
      if (x >= 1) return 1;
      let lo = 0, hi = 1, t = x;
      for (let i = 0; i < 24; i++) {
        const cur = cx(t);
        if (Math.abs(cur - x) < 1e-5) break;
        if (cur < x) lo = t; else hi = t;
        t = (lo + hi) / 2;
      }
      return cy(t);
    };
  }

  const EASE = {
    out: bezier(0.22, 1, 0.36, 1),   // principal
    err: bezier(0.16, 1, 0.3, 1),    // error reveal
    edge: bezier(0.33, 1, 0.68, 1),  // graph edge draw
    inout: bezier(0.65, 0, 0.35, 1),
    linear: (t) => t,
  };

  const clamp01 = (v) => (v < 0 ? 0 : v > 1 ? 1 : v);
  const $ = (sel) => {
    const el = document.querySelector(sel);
    if (!el) throw new Error(`scene: no element for ${sel}`);
    return el;
  };
  const $$ = (sel) => [...document.querySelectorAll(sel)];

  const steps = [];
  let meta = { name: "scene", duration: 10000, fps: 30, width: 1600, height: 900, posterAt: 0 };

  function add(at, dur, apply) {
    steps.push({ at, dur: Math.max(dur, 1), apply });
  }

  const api = {
    define(m) { meta = { ...meta, ...m }; },

    /* Fade/slide/scale an element in (or out with {from:1,to:0}). */
    fade(sel, { at, dur = 400, from = 0, to = 1, y = 0, x = 0, scale = null, ease = "out" } = {}) {
      const el = $(sel);
      const e = EASE[ease];
      add(at, dur, (p) => {
        const q = e(p);
        el.style.opacity = String(from + (to - from) * q);
        const parts = [];
        if (y) parts.push(`translateY(${(1 - q) * y}px)`);
        if (x) parts.push(`translateX(${(1 - q) * x}px)`);
        if (scale !== null) parts.push(`scale(${scale + (1 - scale) * q})`);
        el.style.transform = parts.join(" ");
      });
    },

    /* Type an element's text progressively (text stored at prepare time). */
    type(sel, { at, dur = 900, caret = null } = {}) {
      const el = $(sel);
      const full = el.dataset.full ?? (el.dataset.full = el.textContent);
      el.textContent = "";
      const c = caret ? $(caret) : null;
      add(at, dur, (p) => {
        const n = Math.round(full.length * EASE.linear(p));
        el.textContent = full.slice(0, n);
        if (c) c.classList.toggle("on", p > 0 && p < 1);
      });
    },

    /* Reveal child elements one by one (line-by-line terminal output). */
    lines(sel, { at, stagger = 90, dur = 260, y = 8, ease = "out" } = {}) {
      const els = $$(sel);
      const e = EASE[ease];
      els.forEach((el, i) => {
        add(at + i * stagger, dur, (p) => {
          const q = e(p);
          el.style.opacity = String(q);
          el.style.transform = `translateY(${(1 - q) * y}px)`;
        });
      });
      return at + (els.length - 1) * stagger + dur;
    },

    /* Draw an SVG path (stroke-dash reveal). */
    draw(sel, { at, dur = 700, ease = "edge" } = {}) {
      const el = $(sel);
      const len = el.getTotalLength();
      el.style.strokeDasharray = String(len);
      const e = EASE[ease];
      add(at, dur, (p) => {
        el.style.strokeDashoffset = String(len * (1 - e(p)));
      });
    },

    /* Toggle classes as a binary state change at time t. */
    cls(sel, { at, add: addCls = [], remove = [] } = {}) {
      const els = $$(sel);
      add(at, 1, (p) => {
        for (const el of els) {
          for (const c of addCls) el.classList.toggle(c, p >= 1);
          for (const c of remove) el.classList.toggle(c, p < 1);
        }
      });
    },

    /* Interpolate arbitrary numeric CSS custom property or style. */
    style(sel, prop, { at, dur = 400, from = 0, to = 1, unit = "", ease = "out" } = {}) {
      const el = $(sel);
      const e = EASE[ease];
      add(at, dur, (p) => {
        el.style.setProperty(prop, `${from + (to - from) * e(p)}${unit}`);
      });
    },

    /* Camera: pan/zoom the whole stage content group. */
    camera(sel, { at, dur = 600, fromScale = 1, toScale = 1, fromX = 0, toX = 0, fromY = 0, toY = 0, ease = "inout" } = {}) {
      const el = $(sel);
      const e = EASE[ease];
      add(at, dur, (p) => {
        const q = e(p);
        const s = fromScale + (toScale - fromScale) * q;
        const x = fromX + (toX - fromX) * q;
        const y = fromY + (toY - fromY) * q;
        el.style.transform = `translate(${x}px, ${y}px) scale(${s})`;
      });
    },

    ease: EASE,

    /* Finalize: expose __seek/__scene, run preview loop unless ?render. */
    ready() {
      steps.sort((a, b) => a.at - b.at);
      window.__scene = meta;
      // Cumulative fold: apply only steps that have STARTED (at <= t), in
      // timeline order. State before a step starts = markup/CSS initial
      // state, so anything that fades in must start hidden in the scene.
      window.__seek = (ms) => {
        for (const s of steps) {
          if (ms >= s.at) s.apply(clamp01((ms - s.at) / s.dur));
        }
      };
      window.__seek(0);
      const params = new URLSearchParams(location.search);
      if (!params.has("render")) {
        const t0 = performance.now();
        const loop = () => {
          const t = (performance.now() - t0) % (meta.duration + 1200);
          window.__seek(Math.min(t, meta.duration));
          requestAnimationFrame(loop);
        };
        requestAnimationFrame(loop);
      }
    },
  };

  window.Scene = api;
})();
