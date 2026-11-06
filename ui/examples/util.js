// Shared helpers for the example cards.
window.ex = {
  invoke: (cmd, args) => window.__TAURI__.core.invoke(cmd, args),
  el(html) {
    const t = document.createElement("template");
    t.innerHTML = html.trim();
    return t.content.firstElementChild;
  },
  esc: (s) => String(s ?? "").replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[c]),
  /** Run fn now and every ms. */
  every(ms, fn) {
    fn();
    return setInterval(fn, ms);
  },
};
