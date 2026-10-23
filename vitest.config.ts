import { fileURLToPath, URL } from "node:url";
import { mergeConfig } from "vite";
import { defineConfig } from "vitest/config";
import viteConfig from "./vite.config.ts";

export default mergeConfig(
  viteConfig,
  defineConfig({
    test: {
      environment: "jsdom",
      setupFiles: ["./src/test/setup.ts"],
      include: ["src/**/*.test.{ts,tsx}"],
      restoreMocks: true,
      // The canvas package (Konva in the editor tests) starts fontconfig on the first canvas.
      env: { FONTCONFIG_FILE: fileURLToPath(new URL("./src/test/fonts.conf", import.meta.url)) },
    },
  }),
);
