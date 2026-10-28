// The E2E session: an isolated workspace (settings, save folder, WebView2 profile), the
// msedgedriver matching the installed WebView2 Runtime, tauri-driver, and a WebDriver session
// on the E2E build of Snipt (`pnpm tauri build --debug --no-bundle --features e2e`).
import { type ChildProcess, execFileSync, spawn } from "node:child_process";
import { existsSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { connect } from "node:net";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { remote } from "webdriverio";
import { type SeedCapture, seedPng } from "./png";

const ROOT = fileURLToPath(new URL("../..", import.meta.url));
// Built only by `mise run e2e`, into its own target folder (never a build without the feature).
const APP = join(ROOT, "src-tauri", "target", "e2e", "debug", "snipt.exe");
const DRIVERS = join(ROOT, "e2e", ".drivers");
const WEBVIEW2_CLIENT = "{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}";

/** Hotkeys no other app is likely to hold; `Ctrl+Alt+Shift+` keeps them off the user's keys. */
export const HOTKEYS = {
  region: "Ctrl+Alt+Shift+F7",
  window: "Ctrl+Alt+Shift+F8",
  fullscreen: "Ctrl+Alt+Shift+F9",
} as const;

export interface Workspace {
  root: string;
  saveDir: string;
}

function powershell(script: string): string {
  return execFileSync("powershell.exe", ["-NoLogo", "-NoProfile", "-Command", script], {
    encoding: "utf8",
  }).trim();
}

function webview2Version(): string {
  const keys = [
    `HKLM:\\SOFTWARE\\WOW6432Node\\Microsoft\\EdgeUpdate\\Clients\\${WEBVIEW2_CLIENT}`,
    `HKCU:\\Software\\Microsoft\\EdgeUpdate\\Clients\\${WEBVIEW2_CLIENT}`,
  ];
  for (const key of keys) {
    const version = powershell(`(Get-ItemProperty '${key}' -ErrorAction SilentlyContinue).pv`);
    if (/^\d+\.\d+\.\d+\.\d+$/.test(version)) {
      return version;
    }
  }
  throw new Error("WebView2 Runtime が見つかりません");
}

/** `e2e/.drivers/<version>/msedgedriver.exe`, downloaded once per WebView2 version. */
export function ensureEdgeDriver(): string {
  const version = webview2Version();
  const dir = join(DRIVERS, version);
  const exe = join(dir, "msedgedriver.exe");
  if (!existsSync(exe)) {
    mkdirSync(dir, { recursive: true });
    const zip = join(dir, "edgedriver.zip");
    powershell(
      `$ProgressPreference = 'SilentlyContinue'; ` +
        `Invoke-WebRequest 'https://msedgedriver.microsoft.com/${version}/edgedriver_win64.zip' -OutFile '${zip}'; ` +
        `Expand-Archive -Force '${zip}' '${dir}'; Remove-Item '${zip}'`,
    );
  }
  return exe;
}

function assertNotRunning(): void {
  const running = powershell("@(Get-Process snipt -ErrorAction SilentlyContinue).Count");
  if (running !== "0") {
    throw new Error("Snipt が起動しています。E2E テストの前に終了してください(トレイ → 終了)");
  }
}

/** Kills the Snipt started by this run (the E2E build's path only). */
function killApp(): void {
  powershell(
    `Get-Process snipt -ErrorAction SilentlyContinue | ` +
      `Where-Object { $_.Path -eq '${APP}' } | Stop-Process -Force -ErrorAction SilentlyContinue; exit 0`,
  );
}

export function createWorkspace(captures: readonly SeedCapture[]): Workspace {
  const root = mkdtempSync(join(tmpdir(), "snipt-e2e-"));
  const configDir = join(root, "config");
  const saveDir = join(root, "captures");
  mkdirSync(configDir);
  mkdirSync(saveDir);
  writeFileSync(
    join(configDir, "settings.json"),
    JSON.stringify({
      schemaVersion: 1,
      hotkeys: HOTKEYS,
      fullscreenTarget: "cursorMonitor",
      saveDir,
      autostart: false,
    }),
  );
  for (const capture of captures) {
    writeFileSync(join(saveDir, `Snipt_${capture.id}.png`), seedPng(capture));
  }
  return { root, saveDir };
}

async function waitForPort(port: number, timeoutMs: number): Promise<void> {
  const deadline = Date.now() + timeoutMs;
  for (;;) {
    const open = await new Promise<boolean>((resolve) => {
      const socket = connect(port, "127.0.0.1");
      socket.once("connect", () => resolve(socket.end() && true));
      socket.once("error", () => resolve(false));
    });
    if (open) {
      return;
    }
    if (Date.now() > deadline) {
      throw new Error(`127.0.0.1:${port} で WebDriver が待ち受けていません`);
    }
    await new Promise((r) => setTimeout(r, 200));
  }
}

export interface Session {
  browser: WebdriverIO.Browser;
  workspace: Workspace;
  close(): Promise<void>;
}

export async function startSession(captures: readonly SeedCapture[]): Promise<Session> {
  if (!existsSync(APP)) {
    throw new Error(`${APP} がありません。mise run e2e で実行してください`);
  }
  assertNotRunning();
  const edgeDriver = ensureEdgeDriver();
  const workspace = createWorkspace(captures);

  // Inherited by tauri-driver → msedgedriver → Snipt.
  const env = {
    ...process.env,
    SNIPT_E2E_CONFIG_DIR: join(workspace.root, "config"),
    WEBVIEW2_USER_DATA_FOLDER: join(workspace.root, "webview2"),
  };
  const tauriDriver: ChildProcess = spawn(
    process.env.TAURI_DRIVER ?? "tauri-driver",
    ["--native-driver", edgeDriver],
    { env, stdio: ["ignore", "inherit", "inherit"] },
  );
  let stopping = false;
  tauriDriver.once("exit", (code) => {
    if (!stopping) {
      console.error(`tauri-driver exited early (code ${code})`);
    }
  });
  await new Promise<void>((resolve, reject) => {
    tauriDriver.once("spawn", resolve);
    tauriDriver.once("error", (e) =>
      reject(new Error(`tauri-driver を起動できません(mise run e2e で実行してください): ${e}`)),
    );
  });

  let browser: WebdriverIO.Browser;
  try {
    // tauri-driver (4444) proxies to the msedgedriver it starts (4445); both must listen.
    await waitForPort(4444, 30_000);
    await waitForPort(4445, 30_000);
    browser = await remote({
      hostname: "127.0.0.1",
      port: 4444,
      logLevel: "warn",
      // A retried POST /session would start a second Snipt.
      connectionRetryCount: 0,
      // Starting the app is the slowest command; a stalled one fails instead of hanging.
      connectionRetryTimeout: Number(process.env.SNIPT_E2E_REQUEST_TIMEOUT ?? 60_000),
      capabilities: {
        // tauri-driver speaks classic WebDriver only.
        "wdio:enforceWebDriverClassic": true,
        "tauri:options": { application: APP },
      } as WebdriverIO.Capabilities,
    });
    await browser.setTimeout({ pageLoad: 15_000, script: 15_000 });
  } catch (e) {
    tauriDriver.kill();
    killApp();
    throw e;
  }

  return {
    browser,
    workspace,
    async close() {
      stopping = true;
      await browser.deleteSession().catch(() => undefined);
      tauriDriver.kill();
      killApp();
      if (process.env.SNIPT_E2E_KEEP !== "1") {
        rmSync(workspace.root, { recursive: true, force: true, maxRetries: 5, retryDelay: 500 });
      }
    },
  };
}
