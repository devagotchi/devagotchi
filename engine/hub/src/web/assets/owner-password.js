"use strict";

(function () {
  const token = location.hash.length > 1 ? location.hash.slice(1) : "";
  history.replaceState(null, "", location.pathname);
  const link = document.getElementById("owner-link");
  const submit = document.getElementById("owner-password-submit");
  const status = document.getElementById("owner-password-status");
  if (!link || !submit || !status) return;
  try {
    link.value = decodeURIComponent(token);
  } catch (_) {
    link.value = "";
  }
  if (link.value) {
    submit.disabled = false;
  } else {
    status.textContent = "Open the one-time owner link printed by the hub, then try again.";
  }
})();
