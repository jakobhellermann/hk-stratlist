import data from "@strats";
import mapRooms from "../../../data/map.json";
import { url } from "./url.js";

export const UNSORTED = "unsorted";

/** When the messages were last fetched from Discord. */
export const updated = data.updated;

/** Author user id -> display name. */
export const authors = new Map(Object.entries(data.authors));

/** Room name -> area. */
export const areas = new Map(mapRooms.map((room) => [room.name, room.area]));

/** Room name (or `UNSORTED`) -> posts, newest first. */
export const byRoom = new Map();
for (const post of data.posts.toSorted((a, b) => Date.parse(b.date) - Date.parse(a.date))) {
  for (const room of post.rooms.length ? post.rooms : [UNSORTED]) {
    if (!byRoom.has(room)) byRoom.set(room, []);
    byRoom.get(room).push(post);
  }
}

export const roomHref = (room) => url(`room/${encodeURIComponent(room)}/`);
export const roomTitle = (room) => (room === UNSORTED ? "no room" : room);

/** Non-room tag -> posts, newest first. */
export const byTag = new Map();
for (const posts of [...byRoom.values()]) {
  for (const post of posts) {
    for (const tag of post.tags) {
      if (!byTag.has(tag)) byTag.set(tag, new Set());
      byTag.get(tag).add(post);
    }
  }
}
for (const [tag, posts] of byTag) {
  byTag.set(
    tag,
    [...posts].sort((a, b) => Date.parse(b.date) - Date.parse(a.date)),
  );
}

/** Tag -> URL-safe slug, unique across all tags. */
export const tagSlugs = new Map();
{
  const used = new Set();
  for (const tag of [...byTag.keys()].sort()) {
    const base = tag.replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "") || "tag";
    let slug = base;
    for (let i = 2; used.has(slug); i++) slug = `${base}-${i}`;
    used.add(slug);
    tagSlugs.set(tag, slug);
  }
}

export const tagHref = (tag) => url(`tag/${tagSlugs.get(tag)}/`);

/** Canonical tag -> variants normalized into it. */
export const tagAliases = new Map(Object.entries(data.aliases));
