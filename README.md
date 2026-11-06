# Edge Starter

A starting point for building your own app on the **Corsair XENEON Edge** on macOS, with the touchscreen working properly.

**Tested on macOS 27.2 (Apple silicon), with the Edge connected to the Mac over USB-C** (one cable carries both the display and touch).

## The problem this solves

On a Mac, the Edge's touch panel shows up as a regular USB touchscreen, and macOS maps every touch onto your **main** display. Tap the Edge and you click whatever is in the same spot on your other monitor.

This starter takes over the Edge's touch panel while it's running, reads the touches itself, and delivers them straight into its own window. Taps, drags and text fields work on the Edge, and your mouse never moves.

## What you get

- **Touch on the Edge.** Taps become normal clicks, drags scroll, tapping a text field focuses it so you can type. Raw touches are available too.
- **Opens fullscreen on the Edge**, even with a Retina main display at 2x and the Edge at 1x (window placement with physical pixels gets this wrong).
- **Stays there.** If the window gets knocked out of fullscreen, or you unplug and replug the Edge, it moves back. Touch also picks back up after a replug, and works even if you plug the Edge in after starting the app.
- **A demo page** in `ui/` (buttons, a text field, a scrolling list, a live touch readout) to replace with your own UI.
- **A log** at `~/Library/Logs/edge-starter.log` that says exactly what happened with touch.

Built with [Tauri 2](https://tauri.app) (Rust) and a plain HTML/CSS/JS page. No web framework required, but you can drop one in.

## Quick start

You need Rust, Node.js, and the Xcode Command Line Tools.

```bash
npm install
```

**1. Sign it with your own certificate.** macOS ties the Input Monitoring permission to the app's signature. Unsigned (ad-hoc) builds get a new signature every build, so you'd have to re-allow touch after every rebuild. Find your identity:

```bash
security find-identity -v -p codesigning
```

Then build with it (a free "Apple Development" certificate from Xcode works):

```bash
APPLE_SIGNING_IDENTITY="Apple Development: Your Name (XXXXXXXXXX)" npm run build
```

Or set `bundle.macOS.signingIdentity` in `src-tauri/tauri.conf.json` so you don't have to pass it each time.

**2. Run it.** The app is in `src-tauri/target/release/bundle/macos/Edge Starter.app`. Drag it to Applications and open it.

**3. Allow Input Monitoring.** macOS asks the first time. If it doesn't, go to System Settings > Privacy & Security > Input Monitoring and turn on Edge Starter. Touch turns on within a few seconds, no restart needed.

Before you ship your own app, change `identifier` in `src-tauri/tauri.conf.json` from `com.example.edgestarter` to your own.

## Building your app

Your UI lives in `ui/`. It's an ordinary web page:

- Touches arrive as normal `click` events, so regular buttons and links work.
- For raw touches, listen for `edgetouch` on `window`:

  ```js
  window.addEventListener("edgetouch", (e) => {
    const { kind, x, y } = e.detail; // kind: "down" | "move" | "up"
  });
  ```

- While a finger is down, the pressed element gets the `edge-pressed` class for feedback.
- Call Rust from the page with `window.__TAURI__.core.invoke("command_name")`. Add commands in `src-tauri/src/lib.rs`.

Tips for the Edge's 2560 x 720 screen: keep touch targets at least 44 px, skip hover-only controls (fingers can't hover), and test at exactly 2560 x 720.

## How the touch takeover works (and what bit us)

All of it is in `src-tauri/src/touch.rs`.

- **The touch panel is three devices, not one.** It shows up as a touch digitizer, a mouse-emulation interface, and a vendor interface (USB `0x27c0:0x0859`). The mouse-emulation one is what moves your main pointer, so the app opens each interface separately with exclusive ("seize") access and logs the result for each.
- **Only the mouse-emulation interface needs Input Monitoring.** The other two can be seized without it, which makes it look like touch "half works" when the permission is missing.
- **macOS's permission check can say "denied" when access is granted.** `IOHIDCheckAccess` returned denied while the seize worked fine. So the app doesn't trust the check: it tries the seize and retries every 3 seconds until it works.
- **Old permission records stick around.** If you allowed an earlier unsigned build, macOS keeps a record pinned to that exact build, and newer builds count as not allowed even though the switch looks on. Remove it and allow again:

  ```bash
  tccutil reset ListenEvent com.example.edgestarter
  ```

- **Touch only works while the app is running.** When it quits, the panel goes back to macOS, which sends touches to the main display again.

## Troubleshooting

Check the log first:

```bash
tail -20 ~/Library/Logs/edge-starter.log
```

| Log says | What to do |
| --- | --- |
| `seize interface 0x1/0x2 -> IOReturn 0xe00002e2` | Input Monitoring isn't in effect. Allow it (and reset the old record, above). |
| `Edge touchscreen not found ... still looking` | The Edge isn't connected (or the USB-C cable only carries power). Touch turns on by itself once it's plugged in. |
| `took over every Edge touch interface` | Touch is working. |
| `no Edge-shaped display found` | The Edge isn't connected as a display, so the app opened a normal window. |

## License

MIT. Build whatever you want on it.
