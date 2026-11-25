(function () {
  "use strict";
  var page = document.querySelector('[data-page="audit-history"]');
  if (!page || new URL(location.href).searchParams.has("before")) return;
  var pending = null, stopped = false;
  async function wait() {
    if (stopped || document.hidden || pending) return;
    pending = new AbortController();
    try {
      var response = await fetch(page.dataset.changes, { credentials: "same-origin", cache: "no-store", redirect: "error", signal: pending.signal });
      if (!response.ok) throw new Error("audit unavailable");
      var result = await response.json();
      if (result.changed) {
        stopped = true;
        var draft = Array.prototype.some.call(page.querySelectorAll("form input"), function (input) {
          return input.value !== input.defaultValue;
        });
        if (draft || (document.activeElement && document.activeElement.matches("input, textarea, select"))) {
          document.getElementById("audit-updated").hidden = false;
        } else {
          location.replace(page.dataset.refresh);
        }
      }
    } catch (error) {
      if (error.name !== "AbortError") {
        stopped = true;
        document.getElementById("audit-unavailable").hidden = false;
      }
    } finally {
      pending = null;
      if (!stopped && !document.hidden) setTimeout(wait, 250);
    }
  }
  document.addEventListener("visibilitychange", function () {
    if (document.hidden && pending) pending.abort();
    if (!document.hidden) wait();
  });
  window.addEventListener("pagehide", function () { stopped = true; if (pending) pending.abort(); });
  window.addEventListener("pageshow", function (event) { if (event.persisted) { stopped = false; wait(); } });
  wait();
})();
