// Windows E2E tests (`mise run e2e`): one Snipt session per spec file, scenarios in order.
import { fileURLToPath, URL } from "node:url";
import { defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    root: fileURLToPath(new URL("..", import.meta.url)),
    include: ["e2e/specs/**/*.e2e.ts"],
    environment: "node",
    fileParallelism: false,
    testTimeout: 60_000,
    // Starting the session can include downloading msedgedriver.
    hookTimeout: 180_000,
  },
});
