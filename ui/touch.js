/**
 * touch.js — turns the Edge's touches into normal clicks, scrolls and focus.
 *
 * The Rust side takes over the Edge's touch panel (macOS would otherwise send
 * every touch to your main display) and calls:
 *
 *   window.__edgeTouch("down" | "move" | "up", x, y)   // x, y in window pixels
 *
 * This file handles those calls:
 *   - tap   -> pointer, mouse and click events on whatever is under the finger
 *   - drag  -> scrolls the nearest scrollable element
 *   - tap on a text field -> focuses it and brings the app forward so the
 *     keyboard types into it
 *
 * Mouse and keyboard keep working normally alongside it. Your own code can
 * listen to `edgetouch` events on window for raw touches (see app.js).
 */
(() => {
  const TAP_SLOP = 12; // px a finger can wander before a tap becomes a scroll
  const TEXT_INPUT = "input, textarea, [contenteditable='true']";
  const PRESSABLE = "button, a, label, [role='button'], input, textarea";
  let g = null; // the current gesture

  const invoke = (cmd) => window.__TAURI__?.core?.invoke(cmd).catch(() => {});

  function scrollParent(el) {
    for (let n = el; n; n = n.parentElement) {
      const s = getComputedStyle(n);
      if (/(auto|scroll)/.test(s.overflowY) && n.scrollHeight > n.clientHeight) return n;
    }
    return null;
  }

  function tap(el, x, y) {
    const opts = { bubbles: true, cancelable: true, clientX: x, clientY: y, view: window };
    const touch = { ...opts, pointerType: "touch", isPrimary: true };
    el.dispatchEvent(new PointerEvent("pointerdown", touch));
    el.dispatchEvent(new MouseEvent("mousedown", opts));
    el.dispatchEvent(new PointerEvent("pointerup", touch));
    el.dispatchEvent(new MouseEvent("mouseup", opts));
    if (el instanceof HTMLElement) el.click();
    else el.dispatchEvent(new MouseEvent("click", opts)); // e.g. an SVG icon
  }

  window.__edgeTouch = (kind, x, y) => {
    window.dispatchEvent(new CustomEvent("edgetouch", { detail: { kind, x, y } }));

    if (kind === "down") {
      const target = document.elementFromPoint(x, y);
      const pressed = target?.closest(PRESSABLE) ?? null;
      pressed?.classList.add("edge-pressed");
      g = { x, y, lastY: y, scroller: scrollParent(target), scrolling: false, pressed };
      return;
    }
    if (!g) return;
    if (kind === "move") {
      if (!g.scrolling && Math.hypot(x - g.x, y - g.y) > TAP_SLOP) {
        g.scrolling = true;
        g.pressed?.classList.remove("edge-pressed");
      }
      if (g.scrolling) g.scroller?.scrollBy(0, g.lastY - y);
      g.lastY = y;
      return;
    }
    // up
    g.pressed?.classList.remove("edge-pressed");
    const wasScroll = g.scrolling;
    g = null;
    if (wasScroll) return;
    const target = document.elementFromPoint(x, y);
    if (!target) return;
    const field = target.closest(TEXT_INPUT);
    if (field) {
      invoke("focus_window");
      field.focus();
      return;
    }
    tap(target, x, y);
  };

  window.addEventListener("contextmenu", (e) => e.preventDefault());
})();
