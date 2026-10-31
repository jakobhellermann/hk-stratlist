import { defineConfig } from "astro/config";

export default defineConfig({
  publicDir: "../data/public",
  prefetch: { prefetchAll: true, defaultStrategy: "hover" },
  vite: { server: { fs: { allow: [".."] } } },
});
