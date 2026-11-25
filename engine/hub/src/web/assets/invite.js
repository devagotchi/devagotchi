// The invite page (`web/orgs.rs`): the token after "#" never reaches a server. It is kept in
// this tab's session storage (so it survives signing in first), taken out of the address
// bar and the history, and put in the form when the person is signed in.
(function () {
  "use strict";
  const KEY = "devagotchi-invite";
  try {
    if (location.hash.length > 1) {
      sessionStorage.setItem(KEY, location.hash.slice(1));
      history.replaceState(null, "", location.pathname + location.search);
    }
  } catch (_) {
    // No storage (a private window that refuses it): the token stays in the address bar.
  }
  const field = document.getElementById("invite-token");
  if (!field) return;
  let token = "";
  try {
    token = sessionStorage.getItem(KEY) || "";
  } catch (_) {
    token = location.hash.length > 1 ? location.hash.slice(1) : "";
  }
  if (!token) {
    const missing = document.getElementById("invite-missing");
    const form = document.getElementById("invite-form");
    if (missing) missing.hidden = false;
    if (form) form.hidden = true;
    return;
  }
  field.value = token;
  const form = document.getElementById("invite-form");
  if (form) {
    form.addEventListener("submit", () => {
      try {
        sessionStorage.removeItem(KEY);
      } catch (_) {}
    });
  }
})();
