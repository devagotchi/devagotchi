// The landing page's setup walkthrough (web/landing/walkthrough.rs). Choosing a step works
// without this script (the steps are radio buttons the stylesheet follows); this moves to the
// next step after each step's seconds, draws how long is left on its tab, and lets a click on
// the scene move on at once. It holds still while the person pauses it, while a key moved
// focus into it, while it is off screen or the tab is hidden, and never moves on its own for
// someone who asks for less motion.
(() => {
  const root = document.querySelector("[data-walkthrough]");
  if (!root) return;
  const radios = [...root.querySelectorAll(".walk-tab input")];
  const scenes = [...root.querySelectorAll(".walk-scene")];
  const stage = root.querySelector(".walk-stage");
  const pause = root.querySelector(".walk-pause");
  if (radios.length < 2 || !stage || !pause) return;
  const still = matchMedia("(prefers-reduced-motion: reduce)");
  let timer = 0;
  let bar = null;
  let paused = false;
  let keyed = false;
  let seen = true;

  const current = () => Math.max(0, radios.findIndex((radio) => radio.checked));
  const held = () => paused || keyed || !seen || document.hidden || still.matches;

  function stop() {
    clearTimeout(timer);
    timer = 0;
    if (bar) {
      bar.cancel();
      bar = null;
    }
  }

  // Start the current step's time over: its bar fills, then the next step comes.
  function arm() {
    stop();
    if (held()) return;
    const at = current();
    const seconds = parseFloat(scenes[at] && scenes[at].dataset.seconds) || 4;
    const line = radios[at].parentElement.querySelector(".walk-bar");
    if (line && line.animate) {
      bar = line.animate([{ transform: "scaleX(0)" }, { transform: "scaleX(1)" }], {
        duration: seconds * 1000,
        easing: "linear",
        fill: "forwards",
      });
    }
    timer = setTimeout(() => go(at + 1), seconds * 1000);
  }

  function go(at) {
    radios[(at + radios.length) % radios.length].checked = true;
    arm();
  }

  radios.forEach((radio) => radio.addEventListener("change", arm));
  stage.addEventListener("click", () => go(current() + 1));
  root.addEventListener("focusin", (event) => {
    keyed = event.target.matches(":focus-visible");
    arm();
  });
  root.addEventListener("focusout", (event) => {
    if (!root.contains(event.relatedTarget)) {
      keyed = false;
      arm();
    }
  });
  document.addEventListener("visibilitychange", arm);
  if (still.addEventListener) still.addEventListener("change", arm);
  if ("IntersectionObserver" in window) {
    new IntersectionObserver(([entry]) => {
      seen = entry.isIntersecting;
      arm();
    }).observe(root);
  }
  pause.addEventListener("click", () => {
    paused = !paused;
    pause.textContent = paused ? pause.dataset.play : pause.dataset.pause;
    arm();
  });
  pause.hidden = still.matches;
  root.classList.add("live");
  arm();
})();
