const MIN_W = 5;
const LABEL_PX = 11;
// Rough average glyph width relative to font size.
const CHAR_EM = 0.6;

export function initMap(map) {
  const view = map.viewBox.baseVal;
  const storageKey = `map-view:${map.dataset.game}`;
  const maxW = view.width * 2;
  const labels = [...map.querySelectorAll("text.label")].map((text) => ({
    text,
    chars: text.textContent.length,
    w: Number(text.dataset.w),
    h: Number(text.dataset.h),
  }));

  const saved = sessionStorage.getItem(storageKey);
  if (saved) [view.x, view.y, view.width, view.height] = JSON.parse(saved);

  function toWorld(clientX, clientY) {
    const rect = map.getBoundingClientRect();
    // preserveAspectRatio "xMidYMid meet": uniform scale, content centered.
    const scale = Math.max(view.width / rect.width, view.height / rect.height);
    return {
      x: view.x + view.width / 2 + (clientX - rect.left - rect.width / 2) * scale,
      y: view.y + view.height / 2 + (clientY - rect.top - rect.height / 2) * scale,
      scale,
    };
  }

  // Labels keep a constant on-screen size and only show where they fit inside their room.
  function updateLabels() {
    const fontSize = LABEL_PX * toWorld(0, 0).scale;
    map.style.setProperty("--label-size", `${fontSize}px`);
    for (const { text, chars, w, h } of labels) {
      text.style.display = chars * CHAR_EM * fontSize < w && fontSize < h ? "" : "none";
    }
  }

  let pending = false;
  function viewChanged() {
    if (pending) return;
    pending = true;
    requestAnimationFrame(() => {
      pending = false;
      updateLabels();
      sessionStorage.setItem(storageKey, JSON.stringify([view.x, view.y, view.width, view.height]));
    });
  }

  function zoomAt(clientX, clientY, factor) {
    const clamped = Math.min(Math.max(view.width * factor, MIN_W), maxW) / view.width;
    const p = toWorld(clientX, clientY);
    view.x = p.x - (p.x - view.x) * clamped;
    view.y = p.y - (p.y - view.y) * clamped;
    view.width *= clamped;
    view.height *= clamped;
    viewChanged();
  }

  map.addEventListener(
    "wheel",
    (e) => {
      e.preventDefault();
      zoomAt(e.clientX, e.clientY, Math.exp(e.deltaY * 0.0015));
    },
    { passive: false },
  );

  const pointers = new Map();
  let dragged = false;
  let pinchDist = null;

  map.addEventListener("pointerdown", (e) => {
    pointers.set(e.pointerId, { x: e.clientX, y: e.clientY });
    if (pointers.size === 1) dragged = false;
    pinchDist = null;
  });

  map.addEventListener("pointermove", (e) => {
    const prev = pointers.get(e.pointerId);
    if (!prev) return;
    const cur = { x: e.clientX, y: e.clientY };
    if (pointers.size === 1) {
      const dx = cur.x - prev.x;
      const dy = cur.y - prev.y;
      if (!dragged && Math.hypot(dx, dy) < 3) return;
      // Capture only once dragging, so plain clicks still reach the room links.
      if (!dragged) map.setPointerCapture(e.pointerId);
      dragged = true;
      const { scale } = toWorld(0, 0);
      view.x -= dx * scale;
      view.y -= dy * scale;
      viewChanged();
    } else if (pointers.size === 2) {
      pointers.set(e.pointerId, cur);
      const [a, b] = [...pointers.values()];
      const dist = Math.hypot(a.x - b.x, a.y - b.y);
      if (pinchDist) zoomAt((a.x + b.x) / 2, (a.y + b.y) / 2, pinchDist / dist);
      pinchDist = dist;
      dragged = true;
      return;
    }
    pointers.set(e.pointerId, cur);
  });

  for (const type of ["pointerup", "pointercancel"]) {
    map.addEventListener(type, (e) => {
      pointers.delete(e.pointerId);
      pinchDist = null;
    });
  }

  map.addEventListener(
    "click",
    (e) => {
      if (dragged) e.preventDefault();
    },
    { capture: true },
  );

  new ResizeObserver(viewChanged).observe(map);
  updateLabels();
  map.dataset.ready = "";
}
