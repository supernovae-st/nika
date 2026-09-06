import { gsap } from "gsap";
import { ScrambleTextPlugin } from "gsap/ScrambleTextPlugin";
import { mountWorkflowScene, animateWorkflowScene } from "./commerce-scene.js";
import { FILM, SOURCE_FILM, CUTS } from './film-timing.js';

mountWorkflowScene();

gsap.registerPlugin(ScrambleTextPlugin);

const FILM_DURATION = FILM.duration;

const viewport = document.querySelector("#viewport");
const stage = document.querySelector("#stage");
const scrubber = document.querySelector("#scrubber");
const timecode = document.querySelector("#timecode");
const playButton = document.querySelector("#play-button");
const replayButton = document.querySelector("#replay-button");
const railSteps = [...document.querySelectorAll(".rail-step")];

function fitStage() {
  const bounds = viewport.getBoundingClientRect();
  const scale = Math.min(bounds.width / 1600, bounds.height / 900);
  stage.style.setProperty("--stage-scale", scale.toFixed(4));
}

function setActiveChapter(time) {
  const active = time < FILM.check ? 0 : time < FILM.run ? 1 : time < FILM.result ? 2 : 3;
  railSteps.forEach((step, index) => step.classList.toggle("is-active", index === active));
}

function formatTime(seconds) {
  const whole = Math.min(FILM_DURATION, Math.floor(seconds));
  return `${String(Math.floor(whole / 60)).padStart(2, "0")}:${String(whole % 60).padStart(2, "0")}`;
}

function updateControls() {
  const current = Math.min(FILM_DURATION, timeline.time());
  scrubber.value = String(Math.round((current / FILM_DURATION) * 1000));
  timecode.textContent = `${formatTime(current)} / ${formatTime(FILM_DURATION)}`;
  setActiveChapter(current);
  const intentTime = intro.time();
  if (intentTime >= 4.02 && intentTime <= 6.74) layoutIntentRelations(".intent-relation", false);
}

function setPlayingState(isPlaying) {
  playButton.classList.toggle("is-paused", !isPlaying);
  playButton.setAttribute("aria-label", isPlaying ? "Pause animation" : "Play animation");
}

function scrambleIn(selector, at, stagger = 0.08) {
  document.querySelectorAll(selector).forEach((element, index) => {
    intro.to(
      element,
      {
        scrambleText: {
          text: element.dataset.scramble,
          chars: "01<>/{}[]",
          revealDelay: 0.08,
          speed: 0.55,
        },
        duration: 0.46,
        ease: "none",
      },
      at + index * stagger,
    );
  });
}

function layoutIntentRelations(selector = ".intent-relation", resetStroke = true) {
  const hero = document.querySelector(".intent-hero");
  const svg = document.querySelector(".intent-relations");
  if (!hero || !svg) return;

  const heroRect = hero.getBoundingClientRect();
  const scaleX = heroRect.width / hero.offsetWidth;
  const scaleY = heroRect.height / hero.offsetHeight;
  svg.setAttribute("viewBox", `0 0 ${hero.offsetWidth} ${hero.offsetHeight}`);

  document.querySelectorAll(selector).forEach((path) => {
    const from = hero.querySelector(path.dataset.from);
    const to = hero.querySelector(path.dataset.to);
    if (!from || !to) return;

    const fromRect = from.getBoundingClientRect();
    const toRect = to.getBoundingClientRect();
    const startX = (fromRect.left + fromRect.width / 2 - heroRect.left) / scaleX;
    const startY = (fromRect.bottom - heroRect.top) / scaleY - 1;
    const endX = (toRect.left + toRect.width / 2 - heroRect.left) / scaleX;
    const endY = (toRect.top - heroRect.top) / scaleY + 1;
    const travel = Math.max(36, endY - startY);
    const route = path.dataset.route;
    const handleRatio = route === "verb" ? 0.34 : route === "capability" ? 0.42 : 0.38;
    const handle = Math.max(18, Math.min(72, travel * handleRatio));

    path.setAttribute("d", `M ${startX} ${startY} C ${startX} ${startY + handle}, ${endX} ${endY - handle}, ${endX} ${endY}`);
    if (resetStroke) {
      gsap.set(path, { strokeDasharray: 1, strokeDashoffset: 1 });
    }
  });
}

function flightPosition(tokenSelector, anchorSelector) {
  const host = document.querySelector('.scene-contract').getBoundingClientRect();
  const anchor = document.querySelector(anchorSelector).getBoundingClientRect();
  const token = document.querySelector(tokenSelector);
  const scale = host.width / 1600;
  return {
    x: (anchor.left - host.left + anchor.width / 2) / scale - token.offsetWidth / 2,
    y: (anchor.top - host.top + anchor.height / 2) / scale - token.offsetHeight / 2,
  };
}

const flightAt = (token, anchor) => ({
  x: () => flightPosition(token, anchor).x,
  y: () => flightPosition(token, anchor).y,
});

const sourceTimeline = gsap.timeline({
  paused: true,
  defaults: { ease: "power3.inOut" },
});
const intro = gsap.timeline({ defaults: { ease: "power3.inOut" } });

gsap.set(".scene", { autoAlpha: 0 });
gsap.set(".scene-intents", { autoAlpha: 1 });
gsap.set(".guide-mark", { autoAlpha: 0, scale: 0.6 });
gsap.set(".intro-eyebrow, .intro-title", { autoAlpha: 0, y: 20 });
gsap.set(".intent-card", { autoAlpha: 0, y: 26, scale: 0.96 });
gsap.set(".intent-hero", { width: 900, height: 214 });
gsap.set(".source, .native-tool, .verb-pill", { autoAlpha: 0, y: 10, scale: 0.76, rotationX: -24 });
gsap.set(".tool-family-label, .tool-divider", { autoAlpha: 0, y: 5 });
gsap.set(".intent-relation", { autoAlpha: 0 });
gsap.set(".people-stack img, .intent-context > span, .intent-context > b", { autoAlpha: 0, y: 7, scale: 0.86 });
gsap.set(".intent-capture-status", { autoAlpha: 0, y: 7, scale: 0.9, top: 698 });
gsap.set(".capture-detail", { autoAlpha: 0, x: -5 });
gsap.set(".intent-glass-scan", { autoAlpha: 0, x: 0 });
gsap.set(".intent-dither-trail", { autoAlpha: 0, x: 0 });
gsap.set(".intent-rich", { autoAlpha: 1 });
gsap.set(".intent-rich .word", {
  autoAlpha: 0,
  y: 8,
  color: "#f0f6fc",
  backgroundColor: "rgba(13, 17, 23, 0)",
  fontFamily: "Martian Grotesk",
  fontWeight: 480,
  fontStyle: "normal",
});
gsap.set(".word-app", { borderColor: "rgba(240, 246, 252, 0)", backgroundColor: "rgba(240, 246, 252, 0)" });
gsap.set(".inline-app-icon", { autoAlpha: 0, width: 0, marginRight: 0, scale: 0.4, rotation: -24 });
gsap.set(".intent-word-icon", { autoAlpha: 0, width: 0, marginRight: 0, scale: 0.4, rotation: -24 });
gsap.set(".word-human, .word-signal, .word-action, .word-context, .word-guard", { borderColor: "rgba(240, 246, 252, 0)", paddingLeft: 0, paddingRight: 0 });
gsap.set(".word-with-icon", { paddingLeft: 0, paddingRight: 0 });
gsap.set(".contract-copy > *, .yaml-window", { autoAlpha: 0, y: 28 });
gsap.set(".contract-source-layout", { autoAlpha: 0, scale: 0.985 });
gsap.set(".source-fragment", { autoAlpha: 0, y: 6, clipPath: "inset(0 100% 0 0 round 9px)" });
gsap.set(".source-fragment > small, .source-fragment > b", { autoAlpha: 0, x: -5 });
gsap.set(".code-line", { autoAlpha: 0, x: -32 });
gsap.set(".code-line b", { autoAlpha: 0 });
gsap.set(".compile-axis", { autoAlpha: 0, scaleX: 0.5 });
gsap.set(".compile-editor-lines i", { scaleX: 0 });
gsap.set(".compile-caret", { autoAlpha: 0 });
gsap.set(".flight-token", { autoAlpha: 0, scale: 0.82, left: 0, top: 0 });
gsap.set(".contract-seal", { autoAlpha: 0, scale: 0.8 });
gsap.set(".end-lockup > *", { autoAlpha: 0, y: 20 });

// 00:00. Several understandable outcomes. One becomes the hero.
intro.addLabel("intent", 0);
intro
  .to(".intro-eyebrow", { autoAlpha: 1, y: 0, duration: 0.45 }, 0.16)
  .to(".intro-title", { autoAlpha: 1, y: 0, duration: 0.62 }, 0.38)
  .to(".intent-secondary", { autoAlpha: 0.5, y: 0, scale: 1, duration: 0.62, stagger: 0.14 }, 0.65)
  .to(".intent-hero", { autoAlpha: 1, y: 0, scale: 1, width: 1100, height: 340, duration: 0.72 }, 0.82)
  .to(".people-stack img", { autoAlpha: 1, y: 0, scale: 1, duration: 0.32, stagger: 0.12 }, 1.02)
  .to(".intent-context > span, .intent-context > b", { autoAlpha: 1, y: 0, scale: 1, duration: 0.34, stagger: 0.12 }, 1.4)
  .to(".intent-rich .word", { autoAlpha: 1, y: 0, duration: 0.26, stagger: 0.075, ease: "power2.out" }, 1.22)
  .to(".intro-title", { scrambleText: { text: "Nika structures the intent.", chars: "01<>/{}", revealDelay: 0.06, speed: 0.5 }, color: "#dcecff", duration: 0.66, ease: "none" }, 3.0)
  .to(".intro-eyebrow", { color: "#75869a", duration: 0.3 }, 3.0)
  .to(".intent-secondary", { autoAlpha: 0.16, scale: 0.94, duration: 0.46 }, 3.0)
  .to(".intent-hero", { scale: 1.012, duration: 0.15, yoyo: true, repeat: 1, ease: "power2.out" }, 3.0)
  .to(".prompt-mark", { scale: 1.42, textShadow: "0 0 22px rgba(88, 166, 255, 0.92)", duration: 0.16, yoyo: true, repeat: 1 }, 3.0)
  .to(".intent-capture-status", { autoAlpha: 1, y: 0, scale: 1, duration: 0.36, ease: "back.out(1.7)" }, 3.04)
  .to(".intent-capture-status i", { scale: 1.65, duration: 0.15, yoyo: true, repeat: 1 }, 3.1)
  .to(".capture-detail", { autoAlpha: 1, x: 0, duration: 0.28 }, 3.24)
  .to(".intent-hero", { borderColor: "rgba(159, 208, 255, 0.82)", boxShadow: "0 18px 72px rgba(88, 166, 255, 0.14)", duration: 0.28 }, 3.04)
  .to(".intent-glass-scan", { autoAlpha: 1, x: 1540, duration: 1.26, ease: "power2.inOut" }, 3.08)
  .to(".intent-dither-trail", { autoAlpha: 0.52, x: 1430, duration: 1.18, ease: "power2.inOut" }, 3.14)
  .to(".intent-dither-trail", { autoAlpha: 0, duration: 0.22 }, 4.1)
  .to(".intent-glass-scan", { autoAlpha: 0, duration: 0.18 }, 4.18)
  .to(".word-human", { color: "#f0b35f", backgroundColor: "rgba(240, 179, 95, 0.1)", borderColor: "rgba(240, 179, 95, 0.3)", paddingLeft: 6, paddingRight: 6, fontFamily: "Clash Display", scale: 1.035, duration: 0.34, stagger: 0.42 }, 3.24)
  .to(".word-signal", { color: "#f85149", backgroundColor: "rgba(248, 81, 73, 0.09)", borderColor: "rgba(248, 81, 73, 0.3)", paddingLeft: 6, paddingRight: 6, fontFamily: "Geist Pixel", fontWeight: 400, duration: 0.34 }, 3.38)
  .to(".word-action", { color: "#9fd0ff", backgroundColor: "rgba(159, 208, 255, 0.08)", borderColor: "rgba(159, 208, 255, 0.28)", paddingLeft: 6, paddingRight: 6, fontFamily: "Geist", fontWeight: 780, duration: 0.34, stagger: 0.19 }, 3.44)
  .to(".word-context", { color: "#a78bfa", backgroundColor: "rgba(167, 139, 250, 0.09)", borderColor: "rgba(167, 139, 250, 0.3)", paddingLeft: 6, paddingRight: 6, fontFamily: "Clash Display", scale: 1.035, duration: 0.34 }, 3.6)
  .to(".word-app", { color: "#f0f6fc", fontFamily: "Martian Mono", fontWeight: 760, borderColor: "rgba(159, 208, 255, 0.32)", backgroundColor: "rgba(159, 208, 255, 0.07)", duration: 0.34 }, 3.78)
  .to(".word-with-icon", { paddingLeft: 7, paddingRight: 8, duration: 0.38, ease: "power2.out" }, 3.8)
  .to(".intent-word-icon", { autoAlpha: 1, width: 20, marginRight: 7, scale: 1, rotation: 0, duration: 0.46, stagger: 0.07, ease: "back.out(1.8)" }, 3.84)
  .to(".inline-app-icon", { autoAlpha: 1, width: 20, marginRight: 6, scale: 1, rotation: 0, duration: 0.48, ease: "back.out(1.8)" }, 3.92)
  .to(".word-guard", { color: "#f0b35f", backgroundColor: "rgba(240, 179, 95, 0.1)", borderColor: "rgba(240, 179, 95, 0.3)", paddingLeft: 6, paddingRight: 6, fontFamily: "Geist", fontWeight: 760, scale: 1.025, duration: 0.34, stagger: 0.2 }, 3.9)
  .to(".capture-detail", { scrambleText: { text: "meaning resolved", chars: "01<>/{}", speed: 0.45 }, color: "#9fd0ff", duration: 0.42, ease: "none" }, 4.02)

  .to(".intent-hero", { width: 1240, height: 488, boxShadow: "0 22px 82px rgba(88, 166, 255, 0.17)", duration: 0.68, ease: "expo.inOut" }, 4.06)
  .to(".intent-capture-status", { top: 773, duration: 0.68, ease: "expo.inOut" }, 4.06)
  .to(".mcp-family .tool-family-label", { autoAlpha: 1, y: 0, duration: 0.26 }, 4.2)
  .to(".source-github", { autoAlpha: 1, y: 0, scale: 1, rotationX: 0, duration: 0.4, ease: "back.out(1.7)" }, 4.28)
  .to(".word-app", { scale: 1.09, duration: 0.16, yoyo: true, repeat: 1 }, 4.3)
  .to(".source-notify", { autoAlpha: 1, y: 0, scale: 1, rotationX: 0, duration: 0.4, ease: "back.out(1.7)" }, 4.46)
  .to(".word-notify", { scale: 1.09, duration: 0.16, yoyo: true, repeat: 1 }, 4.48)
  .to(".source-slack", { autoAlpha: 1, y: 0, scale: 1, rotationX: 0, duration: 0.4, ease: "back.out(1.7)" }, 4.64)
  .to(".word-gather", { scale: 1.09, duration: 0.16, yoyo: true, repeat: 1 }, 4.66)
  .to(".source-linear", { autoAlpha: 1, y: 0, scale: 1, rotationX: 0, duration: 0.4, ease: "back.out(1.7)" }, 4.82)
  .to(".word-context", { scale: 1.09, duration: 0.16, yoyo: true, repeat: 1 }, 4.84)
  .call(layoutIntentRelations, [".relation-mcp", true], 4.48)
  .to(".relation-mcp", { autoAlpha: 0.46, strokeDashoffset: 0, duration: 0.5, stagger: 0.14, ease: "power2.inOut" }, 4.5)

  .to(".intent-hero", { boxShadow: "0 26px 94px rgba(88, 166, 255, 0.2)", duration: 0.34 }, 4.92)
  .to(".tool-divider, .builtin-family .tool-family-label", { autoAlpha: 1, y: 0, duration: 0.28 }, 5.02)
  .to(".native-tool", { autoAlpha: 1, y: 0, scale: 1, rotationX: 0, duration: 0.4, stagger: 0.11, ease: "back.out(1.7)" }, 5.12)
  .call(layoutIntentRelations, [".relation-builtin", true], 5.18)
  .to(".relation-builtin", { autoAlpha: 0.42, strokeDashoffset: 0, duration: 0.52, stagger: 0.09, ease: "power2.inOut" }, 5.2)
  .to(".word-guard", { scale: 1.08, duration: 0.16, yoyo: true, repeat: 1 }, 5.38)

  .to(".verb-row > .tool-family-label", { autoAlpha: 1, y: 0, duration: 0.26 }, 5.28)
  .to(".intent-hero", { height: 598, duration: 0.54, ease: "expo.inOut" }, 5.26)
  .to(".intent-capture-status", { top: 832, duration: 0.54, ease: "expo.inOut" }, 5.26)
  .to(".verb-pill", { autoAlpha: 1, y: 0, scale: 1, rotationX: 0, duration: 0.42, stagger: 0.1, ease: "back.out(1.75)" }, 5.36)
  .call(layoutIntentRelations, [".relation-verb", true], 5.48)
  .to(".relation-mcp, .relation-builtin", { autoAlpha: 0.16, duration: 0.34 }, 5.5)
  .to(".relation-verb", { autoAlpha: 0.36, strokeDashoffset: 0, duration: 0.58, stagger: 0.08, ease: "power2.inOut" }, 5.5)
  .to(".intent-secondary", { autoAlpha: 0, x: (index) => (index ? 120 : -120), duration: 0.58 }, 5.7)
  .to(".intro-eyebrow, .intro-title", { autoAlpha: 0, y: -18, duration: 0.46 }, 5.74)
  .to(".intent-hero", { scale: 1.025, borderColor: "rgba(159, 208, 255, 0.62)", duration: 0.52 }, 5.68)
  .to(".intent-relation", { autoAlpha: 0.09, duration: 0.34 }, 6.18);

scrambleIn(".source-github [data-scramble]", 4.3);
scrambleIn(".source-notify [data-scramble]", 4.48);
scrambleIn(".source-slack [data-scramble]", 4.66);
scrambleIn(".source-linear [data-scramble]", 4.84);
scrambleIn(".native-tool [data-scramble]", 5.16, 0.12);
scrambleIn(".verb-pill [data-scramble]", 5.38, 0.12);

// The intention is compiled by meaning into the four contract fields.
intro.addLabel("contract", 6.2);
intro
  .to(".intent-meta, .intent-capture-status, .intent-relations", { autoAlpha: 0, scale: 0.97, duration: 0.28, stagger: 0.03, ease: "power2.in" }, 6.2)
  .to(".intent-context, .intent-input", { autoAlpha: 0, y: -8, scale: 0.98, duration: 0.26, ease: "power2.in" }, 6.26)
  .to(".intent-hero", {
    width: 550,
    height: 540,
    x: -435,
    y: -55,
    padding: 0,
    scale: 1,
    borderRadius: 18,
    borderColor: "rgba(159, 208, 255, 0.38)",
    backgroundColor: "#11161d",
    boxShadow: "0 26px 90px rgba(0, 0, 0, 0.34)",
    duration: 0.76,
    ease: "power3.inOut",
  }, 6.36)
  .to(".contract-source-layout", { autoAlpha: 1, scale: 1, duration: 0.3, ease: "power2.out" }, 6.44)
  .set(".scene-contract", { autoAlpha: 1 }, 6.52)
  .set(".yaml-window", { x: 54, y: 0, scale: 0.96, height: 540 }, 6.52)
  .to(".source-fragment", { autoAlpha: 1, y: 0, clipPath: "inset(0 0% 0 0 round 9px)", duration: 0.34, stagger: 0.1, ease: "power3.out" }, 6.58)
  .to(".source-fragment > small", { autoAlpha: 1, x: 0, duration: 0.22, stagger: 0.1 }, 6.72)
  .to(".source-fragment > b", { autoAlpha: 1, x: 0, duration: 0.24, stagger: 0.1 }, 6.8)
  .to(".yaml-window", { autoAlpha: 1, x: 0, y: 0, scale: 1, duration: 0.5, ease: "expo.out" }, 6.7)
  .to(".code-line", { autoAlpha: 1, x: 0, duration: 0.34, stagger: 0.11 }, 7.14)
  .to(".compile-axis", { autoAlpha: 1, scaleX: 1, duration: 0.42 }, 7.22)
  .to(".compile-editor-lines i", { scaleX: 1, duration: 0.2, stagger: 0.08, ease: "power2.out" }, 7.34)
  .to(".compile-caret", { autoAlpha: 1, duration: 0.12 }, 7.36)
  .to(".compile-caret", { autoAlpha: 0.16, duration: 0.14, repeat: 5, yoyo: true, ease: "none" }, 7.48)

  .to(".source-fragment-intent", { borderColor: "rgba(159, 208, 255, 0.72)", boxShadow: "0 0 24px rgba(88, 166, 255, 0.14)", duration: 0.2 }, 7.7)
  .set('.flight-intent', flightAt('.flight-intent', '.source-fragment-intent > b'), 7.72)
  .to('.source-fragment-intent > b', { autoAlpha: 0.12, duration: 0.18 }, 7.72)
  .to(".flight-intent", { autoAlpha: 1, scale: 1, duration: 0.24 }, 7.72)
  .to(".flight-intent", { ...flightAt('.flight-intent', '.yaml-intent b'), duration: 0.68, ease: "power3.inOut" }, 7.96)
  .to(".yaml-intent", { borderLeftColor: "#9fd0ff", backgroundColor: "rgba(159, 208, 255, 0.07)", duration: 0.2 }, 8.5)
  .to(".yaml-intent b", { autoAlpha: 1, duration: 0.24 }, 8.52)
  .to(".flight-intent", { autoAlpha: 0, scale: 0.8, duration: 0.2 }, 8.52)
  .to('.source-fragment-intent > b', { autoAlpha: 1, duration: 0.2 }, 8.52)

  .to(".source-fragment-permits", { borderColor: "rgba(240, 179, 95, 0.72)", boxShadow: "0 0 24px rgba(240, 179, 95, 0.12)", duration: 0.2 }, 8.56)
  .set('.flight-permits', flightAt('.flight-permits', '.source-fragment-permits > b'), 8.58)
  .to('.source-fragment-permits > b', { autoAlpha: 0.12, duration: 0.18 }, 8.58)
  .to(".flight-permits", { autoAlpha: 1, scale: 1, duration: 0.24 }, 8.58)
  .to(".flight-permits", { ...flightAt('.flight-permits', '.yaml-permits b'), duration: 0.64, ease: "power3.inOut" }, 8.82)
  .to(".yaml-permits", { borderLeftColor: "#f0b35f", backgroundColor: "rgba(240, 179, 95, 0.07)", duration: 0.2 }, 9.3)
  .to(".yaml-permits b", { autoAlpha: 1, duration: 0.24 }, 9.32)
  .to(".flight-permits", { autoAlpha: 0, scale: 0.8, duration: 0.2 }, 9.32)
  .to('.source-fragment-permits > b', { autoAlpha: 1, duration: 0.2 }, 9.32)

  .to(".source-fragment-tasks", { borderColor: "rgba(167, 139, 250, 0.72)", boxShadow: "0 0 24px rgba(167, 139, 250, 0.12)", duration: 0.2 }, 9.36)
  .set('.flight-tasks', flightAt('.flight-tasks', '.source-fragment-tasks > b'), 9.38)
  .to('.source-fragment-tasks > b', { autoAlpha: 0.12, duration: 0.18 }, 9.38)
  .to(".flight-tasks", { autoAlpha: 1, scale: 1, duration: 0.24 }, 9.38)
  .to(".flight-tasks", { ...flightAt('.flight-tasks', '.yaml-tasks b'), duration: 0.64, ease: "power3.inOut" }, 9.62)
  .to(".yaml-tasks", { borderLeftColor: "#a78bfa", backgroundColor: "rgba(167, 139, 250, 0.07)", duration: 0.2 }, 10.1)
  .to(".yaml-tasks b", { autoAlpha: 1, duration: 0.24 }, 10.12)
  .to(".flight-tasks", { autoAlpha: 0, scale: 0.8, duration: 0.2 }, 10.12)
  .to('.source-fragment-tasks > b', { autoAlpha: 1, duration: 0.2 }, 10.12)

  .to(".source-fragment-output", { borderColor: "rgba(63, 185, 80, 0.72)", boxShadow: "0 0 24px rgba(63, 185, 80, 0.12)", duration: 0.2 }, 10.16)
  .set('.flight-output', flightAt('.flight-output', '.source-fragment-output > b'), 10.18)
  .to('.source-fragment-output > b', { autoAlpha: 0.12, duration: 0.18 }, 10.18)
  .to(".flight-output", { autoAlpha: 1, scale: 1, duration: 0.24 }, 10.18)
  .to(".flight-output", { ...flightAt('.flight-output', '.yaml-output b'), duration: 0.64, ease: "power3.inOut" }, 10.42)
  .to(".yaml-output", { borderLeftColor: "#3fb950", backgroundColor: "rgba(63, 185, 80, 0.07)", duration: 0.2 }, 10.9)
  .to(".yaml-output b", { autoAlpha: 1, duration: 0.24 }, 10.92)
  .to(".flight-output", { autoAlpha: 0, scale: 0.8, duration: 0.2 }, 10.92)
  .to('.source-fragment-output > b', { autoAlpha: 1, duration: 0.2 }, 10.92)

  .to(".compile-axis", { autoAlpha: 0, x: -24, duration: 0.38 }, 10.98)
  .to(".intent-hero", { autoAlpha: 0, x: -459, scale: 0.97, duration: 0.38 }, 10.98)
  .to(".contract-seal", { autoAlpha: 1, scale: 1, duration: 0.36 }, 11.06)
  .to(".yaml-window", { borderColor: "rgba(159, 208, 255, 0.5)", duration: 0.18 }, 11.1)
  .to(".yaml-window", { borderColor: "#3a424d", duration: 0.26 }, 11.3);

// Keep the accepted first transition, give the richer request time to read.
intro.duration(25);
sourceTimeline.add(intro, 0);
const workflow = gsap.timeline({ defaults: { ease: 'power3.inOut' } });
animateWorkflowScene(workflow);
sourceTimeline.add(workflow, 8);

sourceTimeline.set({}, {}, SOURCE_FILM.duration);
const timeline = gsap.timeline({ paused: true, onUpdate: updateControls,
  onComplete: () => setPlayingState(false) });
CUTS.slice(1).forEach(([sourceEnd, end], index) => {
  const [, start] = CUTS[index];
  timeline.to(sourceTimeline, { time: sourceEnd, duration: end - start, ease: 'none' }, start);
});

// A frame-accurate public render seam: browser playback and exports use one edit.
window.__film = {
  duration: FILM_DURATION,
  seek(seconds) {
    timeline.pause().time(Math.max(0, Math.min(FILM_DURATION, seconds)), false);
    updateControls();
    setPlayingState(false);
  },
};

playButton.addEventListener("click", () => {
  if (timeline.progress() === 1) timeline.restart();
  else timeline.paused(!timeline.paused());
  setPlayingState(!timeline.paused());
});

replayButton.addEventListener("click", () => {
  timeline.restart();
  setPlayingState(true);
});

scrubber.addEventListener("input", () => {
  timeline.pause().progress(Number(scrubber.value) / 1000);
  setPlayingState(false);
});

document.querySelectorAll('[data-film-time]').forEach(button => {
  button.addEventListener('click', () => {
    timeline.pause().time(Number(button.dataset.filmTime));
    updateControls();
    setPlayingState(false);
  });
});

window.addEventListener("keydown", (event) => {
  if (event.target instanceof HTMLInputElement || event.target instanceof HTMLButtonElement) return;
  if (event.code === "Space") {
    event.preventDefault();
    playButton.click();
  }
  if (event.key.toLowerCase() === "r") replayButton.click();
});

window.addEventListener("resize", () => {
  fitStage();
  if (intro.time() >= 4.7 && intro.time() < 6.7) layoutIntentRelations(".intent-relation", false);
});
fitStage();

const motion = gsap.matchMedia();
motion.add("(prefers-reduced-motion: reduce)", () => {
  timeline.pause().time(FILM.resultFrame);
  updateControls();
  setPlayingState(false);
});
motion.add("(prefers-reduced-motion: no-preference)", () => {
  document.fonts.ready.then(() => {
    if (new URLSearchParams(location.search).has('render')) window.__film.seek(0);
    else { timeline.play(0); setPlayingState(true); }
  });
});
