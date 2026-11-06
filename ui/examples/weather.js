// Weather from Open-Meteo: now, today's high and low, the next hours.
// Set your location in src-tauri/src/examples/weather.rs.
(() => {
  const body = document.querySelector("#weather .body");
  const { invoke } = window.ex;
  const ICON = { clear: "☀︎", partly: "⛅︎", cloudy: "☁︎", fog: "🌫︎", drizzle: "🌦︎", rain: "🌧︎", snow: "❄︎", storm: "⛈︎" };
  const hour = (t) => {
    const h = Number(t.slice(11, 13));
    return `${h % 12 || 12}${h < 12 ? "a" : "p"}`;
  };
  async function render() {
    const w = await invoke("weather").catch(() => null);
    if (!w) {
      body.innerHTML = `<p class="sub">Weather is unavailable right now.</p>`;
      return;
    }
    body.innerHTML = `
      <div class="row"><span class="big-num">${w.temp}°</span><span style="font-size:44px">${ICON[w.kind] ?? ""}</span></div>
      <p class="sub">${w.label} · feels like ${w.feelsLike}° · ${w.high}° / ${w.low}°</p>
      <div class="hours">${w.hours.filter((_, i) => i % 2 === 0).slice(0, 6).map((h) =>
        `<div><span>${hour(h.time)}</span>${ICON[h.kind] ?? ""}<br />${h.temp}°${h.precipChance >= 30 ? `<span>${h.precipChance}%</span>` : ""}</div>`).join("")}</div>`;
  }
  window.ex.every(15 * 60 * 1000, render);
})();
