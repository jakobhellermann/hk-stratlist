import data from "../../../data/strats.json";

export const UNSORTED = "unsorted";

/** Room name -> area. */
export const areas = new Map(Object.entries(data.areas));

/** Room name (or `UNSORTED`) -> posts, newest first. */
export const byRoom = new Map();
for (const post of data.posts.toSorted((a, b) => Date.parse(b.date) - Date.parse(a.date))) {
  for (const room of post.rooms.length ? post.rooms : [UNSORTED]) {
    if (!byRoom.has(room)) byRoom.set(room, []);
    byRoom.get(room).push(post);
  }
}

export const roomHref = (room) => `/room/${encodeURIComponent(room)}/`;
export const roomTitle = (room) => (room === UNSORTED ? "no room" : room);
