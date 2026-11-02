import { fileURLToPath } from "node:url";
import { defineConfig } from "astro/config";

// STRATS_JSON (relative to web/) points the site at another copy of the data, e.g. one with fresh links.
const strats = fileURLToPath(new URL(process.env.STRATS_JSON ?? "../data/strats.json", import.meta.url));

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
