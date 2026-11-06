// Now playing in Spotify or Apple Music, with play/pause, next, previous and
// (for Spotify, once you've signed in) a heart to like the song.
(() => {
  const body = document.querySelector("#now-playing .body");
  const { invoke, esc } = window.ex;
  let liked = null; // { id, on }

  async function render() {
    const np = await invoke("now_playing").catch(() => null);
    if (!np) {
      body.innerHTML = `<p class="sub">Nothing playing in Spotify or Music.</p>`;
      return;
    }
    const spotify = await invoke("spotify_status").catch(() => ({ configured: false }));
    if (np.trackId && spotify.connected && liked?.id !== np.trackId) {
      liked = { id: np.trackId, on: await invoke("spotify_liked", { id: np.trackId }).catch(() => false) };
    }
    const heart = np.app === "Spotify" && np.trackId && spotify.configured
      ? `<button class="pill ${liked?.on ? "on" : ""}" data-act="like">${spotify.connected ? (liked?.on ? "♥ Liked" : "♡ Like") : "Connect Spotify"}</button>`
      : "";
    body.innerHTML = `
      <div class="row">
        ${np.artworkUrl ? `<img class="art" src="${esc(np.artworkUrl)}" alt="" />` : `<div class="art"></div>`}
        <div><p class="title">${esc(np.title)}</p><p class="sub">${esc(np.artist)}</p></div>
      </div>
      <div class="row" style="margin-top:auto">
        <button class="pill" data-act="previous" aria-label="Previous">⏮</button>
        <button class="pill primary" data-act="playpause">${np.playing ? "Pause" : "Play"}</button>
        <button class="pill" data-act="next" aria-label="Next">⏭</button>
        ${heart}
      </div>`;
    body.onclick = async (e) => {
      const act = e.target.closest("[data-act]")?.dataset.act;
      if (!act) return;
      if (act === "like") {
        const s = await invoke("spotify_status");
        if (!s.connected) await invoke("spotify_login").catch((err) => alert(err));
        else {
          const on = !liked?.on;
          liked = { id: np.trackId, on };
          await invoke("spotify_set_liked", { id: np.trackId, liked: on }).catch(() => (liked.on = !on));
        }
      } else {
        await invoke("media_control", { app: np.app, action: act }).catch(() => {});
      }
      setTimeout(render, 300);
    };
  }
  window.ex.every(2000, render);
})();
