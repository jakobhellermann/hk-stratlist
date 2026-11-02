import config from "../../../games.json";
import { url } from "./url.js";

/** Game slug -> contents of `<slug>/<name>`, for each game's file in `dir`. */
const byGame = (files) =>
  new Map(Object.entries(files).map(([path, contents]) => [path.split("/").at(-2), contents]));
const strats = byGame(
  import.meta.glob("@strats/*/strats.json", { eager: true, import: "default" }),
);
const maps = byGame(
  import.meta.glob("../../../data/*/map.json", { eager: true, import: "default" }),
);

/** `{ slug, label }` of every game, in config order. */
export const games = config.games.map(({ slug, label }) => ({ slug, label }));
export const defaultGame = config.default;

export const UNSORTED = "unsorted";
export const roomTitle = (room) => (room === UNSORTED ? "no room" : room);

const cache = new Map();

/** Everything the pages show of one game. */
export function gameData(game) {
  if (!cache.has(game)) cache.set(game, load(game));
  return cache.get(game);
}

function load(game) {
  const data = strats.get(game);
  const mapRooms = maps.get(game);
  if (!data || !mapRooms) throw new Error(`no strats.json or map.json for game ${game}`);
  const gameUrl = (path) => url(`${game}/${path}`);

  /** Room name (or `UNSORTED`) -> posts, newest first. */
  const byRoom = new Map();
  for (const post of data.posts.toSorted((a, b) => Date.parse(b.date) - Date.parse(a.date))) {
    for (const room of post.rooms.length ? post.rooms : [UNSORTED]) {
      if (!byRoom.has(room)) byRoom.set(room, []);
      byRoom.get(room).push(post);
    }
  }

  /** Non-room tag -> posts, newest first. */
  const byTag = new Map();
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
  const tagSlugs = new Map();
  const used = new Set();
  for (const tag of [...byTag.keys()].sort()) {
    const base = tag.replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "") || "tag";
    let slug = base;
    for (let i = 2; used.has(slug); i++) slug = `${base}-${i}`;
    used.add(slug);
    tagSlugs.set(tag, slug);
  }

  return {
    /** When the messages were last fetched from Discord. */
    updated: data.updated,
    /** Author user id -> display name. */
    authors: new Map(Object.entries(data.authors)),
    mapRooms,
    /** Room name -> area. */
    areas: new Map(mapRooms.map((room) => [room.name, room.area])),
    byRoom,
    byTag,
    tagSlugs,
    /** Canonical tag -> variants normalized into it. */
    tagAliases: new Map(Object.entries(data.aliases)),
    /** URL of a page or public file of this game, by its path below the game. */
    gameUrl,
    roomHref: (room) => gameUrl(`room/${encodeURIComponent(room)}/`),
    tagHref: (tag) => gameUrl(`tag/${tagSlugs.get(tag)}/`),
  };
}
