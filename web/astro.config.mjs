import { fileURLToPath } from "node:url";
import { defineConfig } from "astro/config";

// STRATS_DIR (relative to web/) points the site at another copy of the `<game>/strats.json`s, e.g. one with fresh links.
const strats = fileURLToPath(new URL(process.env.STRATS_DIR ?? "../data", import.meta.url));

export default defineConfig({
  site: "https://jakobhellermann.github.io",
  base: "/hk-stratlist",
  publicDir: "../data/public",
  prefetch: { prefetchAll: true, defaultStrategy: "hover" },
  vite: {
    resolve: { alias: { "@strats": strats } },
    server: { fs: { allow: [".."] } },
  },
});
