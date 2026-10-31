const VOLUME = 0.25;
const SEEK_SECONDS = 1;
const SEEK_SECONDS_SHIFT = 5;

/** The video that keyboard shortcuts act on: the one last played or clicked. */
let active = null;

function setup() {
  for (const video of document.querySelectorAll("video")) video.volume = VOLUME;
}
document.addEventListener("astro:page-load", setup);

for (const type of ["play", "pointerdown", "focusin"]) {
  document.addEventListener(
    type,
    (e) => {
      if (e.target instanceof HTMLVideoElement) active = e.target;
    },
    true,
  );
}

// Capture phase with preventDefault, so the native controls' own 5s seek doesn't also fire.
window.addEventListener(
  "keydown",
  (e) => {
    if (!active?.isConnected || e.ctrlKey || e.metaKey || e.altKey) return;
    const frame = 1 / Number(active.dataset.fps);
    let delta;
    switch (e.key) {
      case "ArrowLeft":
        delta = -(e.shiftKey ? SEEK_SECONDS_SHIFT : SEEK_SECONDS);
        break;
      case "ArrowRight":
        delta = e.shiftKey ? SEEK_SECONDS_SHIFT : SEEK_SECONDS;
        break;
      case ",":
        active.pause();
        delta = -frame;
        break;
      case ".":
        active.pause();
        delta = frame;
        break;
      default:
        return;
    }
    e.preventDefault();
    e.stopPropagation();
    active.currentTime = Math.min(
      Math.max(active.currentTime + delta, 0),
      active.duration || Infinity,
    );
  },
  true,
);
