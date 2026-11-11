// The phone app's one script (`web/app/`): registers the service worker (which keeps the
// frame for offline), shows the install banner for iPhone (Share, then Add to Home Screen)
// or Android (the browser's install prompt) while the app is not installed, takes a closed
// ticket's Undo away when its 10 seconds run out, reloads the page when it is pulled down
// from the top, and keeps the count of waiting approvals on the Approvals tab and on the
// installed app's icon. It also moves between the tabs by swipe and opens the Go to sheet
// (below). Everything else it shows is already on the page.
(function () {
  "use strict";
  if ("serviceWorker" in navigator) navigator.serviceWorker.register("/sw.js").catch(function () {});

  var banner = document.getElementById("install");
  var installed = (window.matchMedia && window.matchMedia("(display-mode: standalone)").matches) ||
    window.navigator.standalone === true;
  function show(os) {
    if (!banner || installed) return;
    var line = banner.querySelector("[data-os=" + os + "]");
    if (!line) return;
    line.hidden = false;
    banner.hidden = false;
  }
  if (/iPhone|iPad|iPod/.test(navigator.userAgent)) show("iphone");
  var prompt = null;
  window.addEventListener("beforeinstallprompt", function (event) {
    event.preventDefault();
    prompt = event;
    show("android");
  });
  var go = document.getElementById("install-go");
  if (go) go.addEventListener("click", function () {
    if (!prompt) return;
    prompt.prompt();
    prompt = null;
    banner.hidden = true;
  });
  window.addEventListener("appinstalled", function () { if (banner) banner.hidden = true; });

  // Undo lasts until the time the page gives (the hub refuses it after that anyway).
  var undo = document.getElementById("undo");
  if (undo) {
    var left = Number(undo.dataset.until) - Date.now();
    window.setTimeout(function () { undo.hidden = true; }, Math.max(0, left));
  }

  // How many approvals wait: on the tab, and on the icon where the phone allows it.
  var badge = document.getElementById("approvals-badge");
  function count() {
    if (!badge) return;
    fetch("/app/attention", { credentials: "same-origin" }).then(function (answer) {
      return answer.ok ? answer.json() : null;
    }).then(function (attention) {
      if (!attention) return;
      var waiting = attention.approvals;
      badge.textContent = waiting > 99 ? "99+" : String(waiting);
      badge.hidden = waiting === 0;
      if (navigator.setAppBadge) {
        (waiting ? navigator.setAppBadge(waiting) : navigator.clearAppBadge()).catch(function () {});
      }
    }).catch(function () {});
  }
  count();
  document.addEventListener("visibilitychange", function () {
    if (document.visibilityState === "visible") count();
  });

  // Pull to refresh: the page scrolls inside the frame, so the browser's own pull does not
  // reach it (and an installed app has none).
  var main = document.querySelector("body.app > main");
  if (!main) return;
  var startY = null;
  main.addEventListener("touchstart", function (event) {
    startY = main.scrollTop <= 0 && event.touches.length === 1 ? event.touches[0].clientY : null;
  }, { passive: true });
  main.addEventListener("touchmove", function (event) {
    if (startY === null) return;
    var pulled = event.touches[0].clientY - startY;
    main.style.transform = pulled > 0 ? "translateY(" + Math.min(pulled / 3, 48) + "px)" : "";
    main.dataset.pulled = pulled > 120 ? "yes" : "";
  }, { passive: true });
  main.addEventListener("touchend", function () {
    var reload = main.dataset.pulled === "yes";
    main.style.transform = "";
    main.dataset.pulled = "";
    startY = null;
    if (reload) window.location.reload();
  });
})();

// Moving around the app (`pages.rs` renders the tabs and the Go to sheet): a swipe left or right
// on a tab's own page goes to the next or previous tab and slides that way; a swipe up on the
// tab bar, or the bar's Go to button, opens the sheet; the sheet's sections swipe sideways, and
// a drag down, a tap outside it, its close button or Escape closes it. Every gesture is also a
// tap (WCAG 2.5.1): the tabs, the button, the section chips. Swipes leave the screen's edges to
// the phone (iOS goes Back from there) and never start in something that scrolls sideways.
(function () {
  "use strict";
  var EDGE = 24;
  var tabs = Array.prototype.slice.call(document.querySelectorAll("nav.tabs a"));
  var here = -1;
  tabs.forEach(function (a, i) { if (a.getAttribute("aria-current") === "page") here = i; });
  function remember(way) {
    try { sessionStorage.setItem("dvg-nav", way); } catch (_) {}
    document.documentElement.dataset.nav = way;
  }
  tabs.forEach(function (a, i) {
    a.addEventListener("click", function () {
      if (here >= 0 && i !== here) remember(i > here ? "next" : "prev");
    });
  });

  var main = document.querySelector("body.app > main");
  var onTab = here >= 0 && location.pathname === new URL(tabs[here].href).pathname;
  function sideways(el) {
    for (; el && el !== main && el.nodeType === 1; el = el.parentElement) {
      if (el.matches("[data-no-swipe], input, textarea, select, table, pre")) return true;
      if (el.scrollWidth > el.clientWidth + 1) {
        var overflow = getComputedStyle(el).overflowX;
        if (overflow === "auto" || overflow === "scroll") return true;
      }
    }
    return false;
  }
  if (main && onTab) {
    // On the document, not the page: while a page is still sliding in, touches go to the
    // transition's layer, and a quick second swipe should still count.
    var start = null;
    var settle = function () { main.classList.remove("swiping"); main.style.transform = ""; };
    var inPage = function (event) {
      var box = main.getBoundingClientRect(), touch = event.touches[0];
      var target = event.target;
      return (target === document.documentElement || main.contains(target)) &&
        touch.clientY >= box.top && touch.clientY <= box.bottom;
    };
    document.addEventListener("touchstart", function (event) {
      var touch = event.touches[0];
      start = event.touches.length === 1 && inPage(event) && touch.clientX > EDGE &&
        touch.clientX < window.innerWidth - EDGE && !sideways(event.target)
        ? { x: touch.clientX, y: touch.clientY, at: Date.now(), axis: null } : null;
    }, { passive: true });
    document.addEventListener("touchmove", function (event) {
      if (!start) return;
      var dx = event.touches[0].clientX - start.x, dy = event.touches[0].clientY - start.y;
      if (!start.axis && Math.abs(dx) + Math.abs(dy) > 12) {
        start.axis = Math.abs(dx) > Math.abs(dy) * 1.5 ? "x" : "y";
      }
      if (start.axis !== "x") return;
      var next = tabs[here + (dx < 0 ? 1 : -1)];
      main.classList.add("swiping");
      main.style.transform = "translateX(" + Math.round(dx * (next ? 0.4 : 0.12)) + "px)";
      main.dataset.pulled = "";
    }, { passive: true });
    document.addEventListener("touchend", function (event) {
      if (!start || start.axis !== "x") { start = null; return; }
      var dx = event.changedTouches[0].clientX - start.x;
      var fast = Math.abs(dx) / Math.max(1, Date.now() - start.at) > 0.5;
      var next = tabs[here + (dx < 0 ? 1 : -1)];
      start = null;
      if (next && (Math.abs(dx) > window.innerWidth * 0.25 || (fast && Math.abs(dx) > 48))) {
        remember(dx < 0 ? "next" : "prev");
        window.location.assign(next.href);
        return;
      }
      settle();
    });
    document.addEventListener("touchcancel", function () { start = null; settle(); });
    window.addEventListener("pageshow", settle);
  }

  var sheet = document.getElementById("focus");
  if (!sheet || typeof sheet.showModal !== "function") return;
  var opener = document.getElementById("focus-open");
  var panels = document.getElementById("focus-panels");
  var sections = Array.prototype.slice.call(sheet.querySelectorAll(".sheet-panel"));
  var chips = Array.prototype.slice.call(sheet.querySelectorAll(".sheet-chips a"));
  var filled = false;
  function mark(i) {
    chips.forEach(function (chip, j) {
      if (i === j) chip.setAttribute("aria-current", "true"); else chip.removeAttribute("aria-current");
    });
  }
  function show(i, smooth) {
    panels.scrollTo({ left: panels.clientWidth * i, behavior: smooth ? "smooth" : "auto" });
    mark(i);
  }
  function current() {
    var path = location.pathname, found = 0;
    sections.forEach(function (section, i) {
      if ((section.dataset.tabs || "").split(" ").some(function (p) {
        return p === path || (p !== "/app" && path.indexOf(p + "/") === 0);
      })) found = i;
    });
    return found;
  }
  function list(name, items, empty) {
    var ul = sheet.querySelector("[data-fill=" + name + "]");
    if (!ul) return;
    ul.textContent = "";
    if (!items.length) {
      var none = document.createElement("li");
      none.className = "meta";
      none.textContent = empty;
      ul.appendChild(none);
      return;
    }
    items.forEach(function (item) {
      var li = document.createElement("li"), a = document.createElement("a");
      a.href = item.href;
      a.textContent = item.text;
      li.appendChild(a);
      if (item.meta) {
        var meta = document.createElement("span");
        meta.className = "meta";
        meta.textContent = " " + item.meta;
        li.appendChild(meta);
      }
      ul.appendChild(li);
    });
  }
  function fill() {
    filled = true;
    fetch("/app/focus", { credentials: "same-origin" }).then(function (answer) {
      return answer.ok ? answer.json() : null;
    }).then(function (focus) {
      if (!focus) { filled = false; return; }
      list("boards", focus.boards.map(function (name) {
        return { href: "/app/boards/" + encodeURIComponent(name), text: name };
      }), sheet.dataset.noBoards);
      list("machines", focus.machines.map(function (machine) {
        return { href: "/app/machines/" + encodeURIComponent(machine.name), text: machine.name, meta: machine.status };
      }), sheet.dataset.noMachines);
      if (focus.owner) sheet.querySelectorAll("[data-owner]").forEach(function (el) { el.hidden = false; });
    }).catch(function () { filled = false; });
  }
  function open() {
    if (sheet.open) return;
    sheet.showModal();
    show(current(), false);
    if (!filled) fill();
  }
  if (opener) opener.addEventListener("click", open);
  chips.forEach(function (chip, i) {
    chip.addEventListener("click", function (event) { event.preventDefault(); show(i, true); });
  });
  var frame = 0;
  panels.addEventListener("scroll", function () {
    if (frame) return;
    frame = window.requestAnimationFrame(function () {
      frame = 0;
      mark(Math.round(panels.scrollLeft / Math.max(1, panels.clientWidth)));
    });
  }, { passive: true });
  sheet.addEventListener("click", function (event) { if (event.target === sheet) sheet.close(); });

  // Drag the sheet down by its top (grip, title, chips) to close it.
  var drag = null;
  sheet.addEventListener("touchstart", function (event) {
    drag = event.target.closest(".sheet-grip, .sheet-head, .sheet-chips") && event.touches.length === 1
      ? { y: event.touches[0].clientY, dy: 0 } : null;
  }, { passive: true });
  sheet.addEventListener("touchmove", function (event) {
    if (!drag) return;
    drag.dy = Math.max(0, event.touches[0].clientY - drag.y);
    sheet.classList.add("dragging");
    sheet.style.transform = "translateY(" + drag.dy + "px)";
  }, { passive: true });
  function release() {
    if (!drag) return;
    var gone = drag.dy > 90;
    drag = null;
    sheet.classList.remove("dragging");
    sheet.style.transform = "";
    if (gone) sheet.close();
  }
  sheet.addEventListener("touchend", release);
  sheet.addEventListener("touchcancel", release);

  // Swipe up on the tab bar to open it.
  var bar = document.querySelector("nav.tabs"), up = null;
  if (bar) {
    bar.addEventListener("touchstart", function (event) {
      up = event.touches.length === 1 ? { x: event.touches[0].clientX, y: event.touches[0].clientY } : null;
    }, { passive: true });
    bar.addEventListener("touchend", function (event) {
      if (!up) return;
      var touch = event.changedTouches[0], dy = touch.clientY - up.y, dx = touch.clientX - up.x;
      up = null;
      if (dy < -40 && Math.abs(dx) < 40) open();
    });
  }
})();
