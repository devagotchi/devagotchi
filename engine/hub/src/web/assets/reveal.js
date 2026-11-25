// Runs in the head of every page of the phone app, before it is first drawn: which way the
// page slides in (`app.js` keeps it when a person swipes or taps to another tab), for the
// cross-document view transition in `hub.css`. Nothing else.
(function () {
  "use strict";
  try {
    var way = sessionStorage.getItem("dvg-nav");
    sessionStorage.removeItem("dvg-nav");
    if (way === "next" || way === "prev") document.documentElement.dataset.nav = way;
  } catch (_) {}
})();
