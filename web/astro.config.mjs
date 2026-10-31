import { defineConfig } from "astro/config";

export default defineConfig({
  site: "https://jakobhellermann.github.io",
  base: "/hk-stratlist",
  publicDir: "../data/public",
  prefetch: { prefetchAll: true, defaultStrategy: "hover" },
  vite: { server: { fs: { allow: [".."] } } },
});
