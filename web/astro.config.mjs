import { fileURLToPath } from "node:url";
import { defineConfig } from "astro/config";
import games from "../games.json" with { type: "json" };

// STRATS_DIR (relative to web/) points the site at another copy of the `<game>/strats.json`s, e.g. one with fresh links.
const strats = fileURLToPath(new URL(process.env.STRATS_DIR ?? "../data", import.meta.url));

const base = "/hk-stratlist";

export default defineConfig({
  site: "https://jakobhellermann.github.io",
  base,
  publicDir: "../data/public",
  // Redirect destinations don't get the base prepended.
  redirects: { "/": `${base}/${games.default}/` },
  prefetch: { prefetchAll: true, defaultStrategy: "hover" },
  vite: {
    resolve: { alias: { "@strats": strats } },
    server: { fs: { allow: [".."] } },
  },
});
