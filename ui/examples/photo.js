// Apple Photos: a random Favorite every 30 seconds, fading over the last one.
// Tap the photo for its date and a heart to (un)favorite it in Photos.
(() => {
  const col = document.querySelector("#photo .body");
  const { invoke, esc } = window.ex;
  let current = null;
  let hideTimer;

  const chrome = ex.el(`<div class="chrome"><button class="pill heart" aria-label="Favorite"></button><span class="date"></span></div>`);

  function showChrome(on) {
    chrome.classList.toggle("on", on);
    clearTimeout(hideTimer);
    if (on) hideTimer = setTimeout(() => showChrome(false), 5000);
  }
  function paintChrome() {
    if (!current) return;
    chrome.querySelector(".heart").textContent = current.favorite ? "♥" : "♡";
    chrome.querySelector(".date").textContent = current.taken
      ? new Date(current.taken).toLocaleDateString("en-US", { month: "short", day: "numeric", year: "numeric" }) : "";
  }

  async function next() {
    const r = await invoke("photos_random").catch((e) => ({ error: String(e) }));
    if (r.access === "unknown") {
      col.innerHTML = `<div class="center"><div><button class="pill primary" data-allow>Show my photos</button><p class="sub">macOS will ask once.</p></div></div>`;
      col.querySelector("[data-allow]").onclick = async () => { await invoke("photos_request_access"); next(); };
      return;
    }
    if (!r.photo) {
      col.innerHTML = `<div class="center sub">${r.access === "denied" ? "Allow Photos in System Settings > Privacy & Security > Photos." : esc(r.error ?? "No photos found. Mark some as Favorites.")}</div>`;
      return;
    }
    if (!col.contains(chrome)) {
      col.innerHTML = "";
      col.appendChild(chrome);
    }
    current = r.photo;
    const img = new Image();
    img.alt = "A photo from your library";
    img.onload = () => {
      col.insertBefore(img, chrome);
      requestAnimationFrame(() => img.classList.add("shown"));
      // Keep only the last two images (old one under the new one while it fades).
      const imgs = col.querySelectorAll("img");
      if (imgs.length > 2) imgs[0].remove();
    };
    img.src = current.image;
    paintChrome();
  }

  col.addEventListener("click", async (e) => {
    if (e.target.closest(".heart") && current) {
      current.favorite = !current.favorite;
      paintChrome();
      await invoke("photo_set_favorite", { asset: current.asset, favorite: current.favorite }).catch(() => {
        current.favorite = !current.favorite;
        paintChrome();
      });
      showChrome(true);
      return;
    }
    if (e.target.tagName === "IMG") showChrome(!chrome.classList.contains("on"));
  });

  window.ex.every(30000, () => { if (!chrome.classList.contains("on")) next(); });
})();
