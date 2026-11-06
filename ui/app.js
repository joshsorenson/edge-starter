// Demo wiring. Everything here is ordinary DOM code: touches arrive as clicks.

const count = document.getElementById("count");
document.getElementById("count-btn").addEventListener("click", () => {
  count.textContent = String(Number(count.textContent) + 1);
});

const colors = ["#a394ff", "#5eead4", "#fca5a5", "#fcd34d"];
let c = 0;
document.getElementById("accent-btn").addEventListener("click", () => {
  c = (c + 1) % colors.length;
  document.documentElement.style.setProperty("--accent", colors[c]);
});

const list = document.getElementById("list");
for (let i = 1; i <= 40; i++) {
  const li = document.createElement("li");
  li.textContent = `Item ${i}`;
  list.appendChild(li);
}

// Raw touches, if you want them: { kind: "down" | "move" | "up", x, y }.
const readout = document.getElementById("last-touch");
window.addEventListener("edgetouch", (e) => {
  const { x, y } = e.detail;
  readout.textContent = `${Math.round(x)}, ${Math.round(y)}`;
});

// Touch status from the Rust side.
const status = document.getElementById("touch-status");
const MESSAGES = {
  on: "Touch is on.",
  starting: "Starting touch…",
  needsPermission: "Touch is off: allow Edge Starter in System Settings > Privacy & Security > Input Monitoring.",
  unsupported: "Touch takeover only works on macOS.",
};
async function refreshStatus() {
  const s = await window.__TAURI__?.core?.invoke("touch_status").catch(() => null);
  if (!s) return;
  status.textContent = MESSAGES[s.state] ?? `Touch is off (${s.detail ?? s.state}).`;
  status.classList.toggle("off", s.state !== "on");
}
refreshStatus();
setInterval(refreshStatus, 3000);
