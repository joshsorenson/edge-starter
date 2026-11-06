// This Mac: CPU, memory and network, sampled every 2 seconds by the Rust side.
(() => {
  const body = document.querySelector("#system .body");
  const { invoke } = window.ex;
  const rate = (n) => (n >= 1048576 ? `${(n / 1048576).toFixed(1)} MB/s` : `${Math.round(n / 1024)} KB/s`);
  const spark = (vals, max) => {
    if (vals.length < 2) return "";
    const top = max ?? Math.max(1, ...vals);
    const pts = vals.map((v, i) => `${(i / (vals.length - 1)) * 100},${100 - (v / top) * 96}`).join(" ");
    return `<svg class="chart" viewBox="0 0 100 100" preserveAspectRatio="none"><polyline points="${pts}" fill="none" stroke="var(--accent)" stroke-width="2" vector-effect="non-scaling-stroke" /></svg>`;
  };
  async function render() {
    const s = await invoke("system_snapshot").catch(() => null);
    const last = s?.samples?.[s.samples.length - 1];
    if (!last) {
      body.innerHTML = `<p class="sub">Measuring…</p>`;
      return;
    }
    body.innerHTML = `
      <p class="big-num">${Math.round(last.cpu)}%</p><p class="sub">CPU · ${s.cores.length} cores</p>
      ${spark(s.samples.map((x) => x.cpu), 100)}
      <div class="stat"><span>Memory</span><b>${Math.round((last.memUsed / s.memTotal) * 100)}%</b></div>
      <div class="stat"><span>Down</span><b>${rate(last.down)}</b></div>
      <div class="stat"><span>Up</span><b>${rate(last.up)}</b></div>`;
  }
  window.ex.every(2000, render);
})();
