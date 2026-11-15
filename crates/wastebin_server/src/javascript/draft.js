// Keep the new-paste form's draft in this tab's sessionStorage, see `WASTEBIN_DRAFT_AUTOSAVE`.
//
// On the editor the text, title, extension and expiry are saved while typing and restored when
// the editor loads again, for example after a reload or a sign-in round trip. The password and
// the encryption and burn toggles are never stored. Submitting the form leaves a marker behind;
// the page a successful submission lands on (a paste, the burn notice or the password prompt of
// an encrypted paste) removes the draft. Any other page, such as an error, keeps it.
(function() {
  "use strict";

  const DRAFT_KEY = "wastebin-draft";
  const SUBMITTED_KEY = "wastebin-draft-submitted";
  const DELAY_MS = 500;
  // A submission that has not landed on a paste within this time is considered abandoned.
  const SUBMIT_WINDOW_MS = 10 * 60 * 1000;

  function storage() {
    try {
      return window.sessionStorage;
    } catch (e) {
      return null;
    }
  }

  const store = storage();
  if (!store) return;

  function get(key) {
    try { return store.getItem(key); } catch (e) { return null; }
  }

  function set(key, value) {
    try { store.setItem(key, value); } catch (e) { /* full or disabled */ }
  }

  function remove(key) {
    try { store.removeItem(key); } catch (e) { /* disabled */ }
  }

  const form = document.getElementById("form");

  if (!form || form.getAttribute("action") !== "/new") {
    // Only pages a successful submission lands on load this script without the editor: the
    // paste exists now, so the draft is done.
    const submitted = parseInt(get(SUBMITTED_KEY), 10);
    if (!isNaN(submitted)) {
      if (Date.now() - submitted < SUBMIT_WINDOW_MS) remove(DRAFT_KEY);
      remove(SUBMITTED_KEY);
    }
    return;
  }

  const text = form.elements.namedItem("text");
  const title = form.elements.namedItem("title");
  const extension = form.elements.namedItem("extension");

  function checkedExpiry() {
    const radio = form.querySelector('input[name="expires"]:checked');
    return radio ? radio.value : null;
  }

  function save() {
    if (!text.value && !title.value) {
      remove(DRAFT_KEY);
      return;
    }

    set(DRAFT_KEY, JSON.stringify({
      text: text.value,
      title: title.value,
      extension: extension ? extension.value : null,
      expires: checkedExpiry(),
    }));
  }

  let timer = null;

  function scheduleSave() {
    clearTimeout(timer);
    timer = setTimeout(save, DELAY_MS);
  }

  function saveNow() {
    clearTimeout(timer);
    save();
  }

  function restore() {
    const raw = get(DRAFT_KEY);
    if (!raw) return;

    let draft;
    try {
      draft = JSON.parse(raw);
    } catch (e) {
      remove(DRAFT_KEY);
      return;
    }

    // Never overwrite what the browser or the user already put into the editor.
    if (text.value || typeof draft !== "object" || draft === null) return;

    if (typeof draft.title === "string") {
      title.value = draft.title;
    }

    if (extension && typeof draft.extension === "string") {
      for (const option of extension.options) {
        if (option.value === draft.extension) {
          option.selected = true;
          try { option.scrollIntoView({ block: "nearest" }); } catch (e) { /* old browser */ }
          break;
        }
      }
    }

    if (typeof draft.expires === "string") {
      for (const radio of form.querySelectorAll('input[name="expires"]')) {
        if (radio.value === draft.expires && !radio.disabled) {
          radio.checked = true;
        }
      }
    }

    if (typeof draft.text === "string") {
      text.value = draft.text;
      // Let the editor update line numbers and statistics.
      text.dispatchEvent(new Event("input", { bubbles: true }));
    }
  }

  function markSubmitted() {
    saveNow();
    set(SUBMITTED_KEY, String(Date.now()));
  }

  // Back on the editor, so whatever was submitted last did not produce a paste.
  remove(SUBMITTED_KEY);
  restore();

  form.addEventListener("input", scheduleSave);
  form.addEventListener("change", scheduleSave);
  window.addEventListener("pagehide", saveNow);

  form.addEventListener("submit", function(e) {
    if (!e.defaultPrevented) markSubmitted();
  });

  // The keyboard shortcuts call `form.submit()`, which does not fire a submit event.
  text.addEventListener("keydown", function(e) {
    if ((e.ctrlKey || e.metaKey) && (e.key === "s" || e.key === "Enter")) {
      markSubmitted();
    }
  }, true);
})();
