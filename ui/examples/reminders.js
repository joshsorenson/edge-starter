// Apple Reminders: open ones from every list (due first), tap to check off,
// type to add to your default list. macOS asks once for access.
(() => {
  const body = document.querySelector("#reminders .body");
  const { invoke, esc } = window.ex;
  async function render() {
    const r = await invoke("reminders_list").catch(() => null);
    if (!r) return;
    if (r.access === "unknown") {
      body.innerHTML = `<button class="pill primary" data-allow>Show my Reminders</button><p class="sub">macOS will ask once.</p>`;
      body.querySelector("[data-allow]").onclick = async () => { await invoke("reminders_request_access"); render(); };
      return;
    }
    if (r.access === "denied") {
      body.innerHTML = `<p class="sub">Allow Reminders in System Settings > Privacy & Security > Reminders.</p>`;
      return;
    }
    body.innerHTML = `
      <input placeholder="Add a reminder and press Enter" />
      <ul class="list">${r.reminders.map((x) => `
        <li data-id="${esc(x.id)}" data-done="${x.completed}" class="${x.completed ? "done" : ""}">
          <span class="ring"></span><span>${esc(x.title)}<small>${esc([x.due, x.list].filter(Boolean).join(" · "))}</small></span>
        </li>`).join("") || `<li class="sub">All caught up.</li>`}</ul>`;
    body.querySelector("input").onkeydown = async (e) => {
      if (e.key !== "Enter" || !e.target.value.trim()) return;
      await invoke("reminder_add", { title: e.target.value });
      render();
    };
    body.querySelector(".list").onclick = async (e) => {
      const li = e.target.closest("li[data-id]");
      if (!li) return;
      li.classList.toggle("done");
      await invoke("reminder_set_completed", { id: li.dataset.id, completed: li.dataset.done !== "true" });
      render();
    };
  }
  window.ex.every(15000, render);
})();
