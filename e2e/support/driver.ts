// The E2E session: an isolated workspace (settings, save folder, WebView2 profile), the
// msedgedriver matching the installed WebView2 Runtime, tauri-driver, and a WebDriver session
// on the E2E build of Snipt (`pnpm tauri build --debug --no-bundle --features e2e`).
import { type ChildProcess, execFileSync, spawn } from "node:child_process";
import { existsSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { createServer } from "node:net";
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
/** Attempts at starting the drivers and the session before the run fails. */
const START_ATTEMPTS = 5;
/** Ports for the drivers: clear of the OS's dynamic range, where other processes churn. */
const PORT_MIN = 20000;
const PORT_MAX = 45000;

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

/** Whether the E2E build is still running. */
export function appRunning(): boolean {
  return (
    powershell(
      `@(Get-Process snipt -ErrorAction SilentlyContinue | Where-Object { $_.Path -eq '${APP}' }).Count`,
    ) !== "0"
  );
}

/** Asks the running E2E build to quit, as the tray's 終了 does (`on_second_instance`). */
export function requestQuit(): void {
  execFileSync(APP, ["--e2e-quit"], { timeout: 30_000 });
}

/**
 * Sends a Chrome DevTools Protocol command to the current window's page through msedgedriver
 * (`/ms/cdp/execute`). Used for IME composition, which key input cannot produce.
 */
export async function sendCdp(
  browser: WebdriverIO.Browser,
  cmd: string,
  params: Record<string, unknown>,
): Promise<void> {
  const response = await fetch(
    `http://127.0.0.1:${browser.options.port}/session/${browser.sessionId}/ms/cdp/execute`,
    {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ cmd, params }),
    },
  );
  if (!response.ok) {
    throw new Error(`CDP ${cmd} failed (${response.status}): ${await response.text()}`);
  }
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

/** Whether the port can be bound on 127.0.0.1 right now. */
function bindable(port: number): Promise<boolean> {
  return new Promise((resolve) => {
    const server = createServer();
    server.once("error", () => resolve(false));
    server.listen(port, "127.0.0.1", () => server.close(() => resolve(true)));
  });
}

/**
 * A random port in the dynamic-free range that nothing holds right now. Each session gets its
 * own pair. `listen(0)` is not used: it draws from the OS's dynamic range, where other processes
 * churn and tauri-driver then fails to bind.
 */
async function freePort(): Promise<number> {
  for (let i = 0; i < 100; i++) {
    const port = PORT_MIN + Math.floor(Math.random() * (PORT_MAX - PORT_MIN + 1));
    if (await bindable(port)) {
      return port;
    }
  }
  throw new Error("空きポートが見つかりません");
}

/** Kills the process and its children (msedgedriver under tauri-driver), so none is orphaned. */
function killTree(child: ChildProcess): void {
  if (child.pid === undefined) {
    return;
  }
  try {
    execFileSync("taskkill", ["/PID", String(child.pid), "/T", "/F"], { stdio: "ignore" });
  } catch {
    // Already gone, or taskkill refused: the direct kill still covers the driver itself.
    child.kill();
  }
}

/**
 * Whether a driver answers HTTP on the port. An HTTP status request, not a bare TCP connect: a
 * port that accepts connections is not yet a driver that can start a session.
 */
async function driverReady(port: number): Promise<boolean> {
  try {
    const response = await fetch(`http://127.0.0.1:${port}/status`, {
      signal: AbortSignal.timeout(2_000),
    });
    return response.ok;
  } catch {
    return false;
  }
}

/** The lines a driver prints when its port is taken: tauri-driver's, and msedgedriver's bind error. */
const PORT_TAKEN = /can not listen to address|bind\(\) returned an error/;

/**
 * Kills the drivers one attempt started, found by the ports on their command lines (the ports are
 * fresh to the attempt). Covers a copy that is not a child of the tauri-driver killed by `killTree`,
 * and an msedgedriver whose parent has already exited.
 */
function killByPorts(webdriverPort: number, nativePort: number): void {
  powershell(
    `Get-CimInstance Win32_Process | Where-Object { ` +
      `($_.Name -eq 'tauri-driver.exe' -and $_.CommandLine -match '--port ${webdriverPort}( |$)') -or ` +
      `($_.Name -eq 'msedgedriver.exe' -and $_.CommandLine -match '--port=${nativePort}( |$)') ` +
      `} | ForEach-Object { Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }; exit 0`,
  );
}

/**
 * Waits for the driver to answer. Checks `failure` first on every round, so a driver that cannot
 * serve (a port taken, a process exited) fails at once, even when another process answers HTTP on
 * its port.
 */
async function waitForDriver(
  port: number,
  name: string,
  timeoutMs: number,
  failure: () => string | null,
): Promise<void> {
  const deadline = Date.now() + timeoutMs;
  for (;;) {
    const reason = failure();
    if (reason !== null) {
      throw new Error(`${name} (127.0.0.1:${port}): ${reason}`);
    }
    if (await driverReady(port)) {
      return;
    }
    if (Date.now() > deadline) {
      throw new Error(`${name} が 127.0.0.1:${port} で応答しません`);
    }
    await new Promise((r) => setTimeout(r, 200));
  }
}

export interface Session {
  browser: WebdriverIO.Browser;
  workspace: Workspace;
  close(): Promise<void>;
}

/** One attempt: tauri-driver on fresh ports, then a WebDriver session on the E2E build. */
async function startOnce(
  workspace: Workspace,
  edgeDriver: string,
  attempt: number,
): Promise<Session> {
  const webdriverPort = await freePort();
  let nativePort = await freePort();
  while (nativePort === webdriverPort) {
    nativePort = await freePort();
  }

  // Inherited by tauri-driver → msedgedriver → Snipt.
  const env = {
    ...process.env,
    SNIPT_E2E_CONFIG_DIR: join(workspace.root, "config"),
    WEBVIEW2_USER_DATA_FOLDER: join(workspace.root, "webview2"),
  };
  const tauriDriver: ChildProcess = spawn(
    process.env.TAURI_DRIVER ?? "tauri-driver",
    [
      "--port",
      String(webdriverPort),
      "--native-port",
      String(nativePort),
      "--native-driver",
      edgeDriver,
    ],
    { env, stdio: ["ignore", "inherit", "pipe"] },
  );
  // Why this attempt cannot serve a session: a port taken (tauri-driver reports it and then
  // carries on or exits with 0; msedgedriver reports its bind error while tauri-driver lives), or
  // a driver that exited. Both are read as they happen, not only when the process ends.
  const state: { stopping: boolean; exited: boolean; code: number | null; taken: string | null } = {
    stopping: false,
    exited: false,
    code: null,
    taken: null,
  };
  tauriDriver.stderr?.on("data", (chunk: Buffer) => {
    const text = chunk.toString();
    process.stderr.write(text);
    const line = text.split("\n").find((l) => PORT_TAKEN.test(l));
    if (line !== undefined && state.taken === null) {
      state.taken = line.trim();
    }
  });
  tauriDriver.once("exit", (code) => {
    state.exited = true;
    state.code = code ?? -1;
    if (!state.stopping) {
      console.error(`tauri-driver exited early (code ${code})`);
    }
  });
  await new Promise<void>((resolve, reject) => {
    tauriDriver.once("spawn", resolve);
    tauriDriver.once("error", (e) =>
      reject(new Error(`tauri-driver を起動できません(mise run e2e で実行してください): ${e}`)),
    );
  });
  const failure = (): string | null => {
    if (state.taken !== null) return `port in use: ${state.taken}`;
    if (state.exited) return `exited with code ${state.code}`;
    return null;
  };

  let browser: WebdriverIO.Browser;
  try {
    // tauri-driver answers once it listens; msedgedriver answers once it is ready to start a
    // session. Both are checked over HTTP before the session is requested.
    await waitForDriver(webdriverPort, "tauri-driver", 30_000, failure);
    await waitForDriver(nativePort, "msedgedriver", 30_000, failure);
    browser = await remote({
      hostname: "127.0.0.1",
      port: webdriverPort,
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
    // Accepted only while both drivers are still up and no port was reported taken.
    const late = failure();
    if (late !== null) {
      throw new Error(`driver failed while the session started: ${late}`);
    }
  } catch (e) {
    const reason = failure();
    state.stopping = true;
    killTree(tauriDriver);
    killByPorts(webdriverPort, nativePort);
    killApp();
    if (reason === null) {
      throw e;
    }
    throw new Error(`127.0.0.1:${webdriverPort} / ${nativePort}: ${reason}`, { cause: e });
  }

  // stderr, not console: vitest prints console output only for a failing file, and the line must
  // show in every run.
  process.stderr.write(
    `E2E: session started (attempt ${attempt}/${START_ATTEMPTS}, 127.0.0.1:${webdriverPort}/${nativePort})\n`,
  );
  return {
    browser,
    workspace,
    async close() {
      state.stopping = true;
      await browser.deleteSession().catch(() => undefined);
      const exited = new Promise<void>((resolve) => {
        if (state.exited) resolve();
        else tauriDriver.once("exit", () => resolve());
      });
      killTree(tauriDriver);
      await exited;
      killByPorts(webdriverPort, nativePort);
      killApp();
      if (process.env.SNIPT_E2E_KEEP !== "1") {
        rmSync(workspace.root, { recursive: true, force: true, maxRetries: 5, retryDelay: 500 });
      }
    },
  };
}

export async function startSession(captures: readonly SeedCapture[]): Promise<Session> {
  if (!existsSync(APP)) {
    throw new Error(`${APP} がありません。mise run e2e で実行してください`);
  }
  assertNotRunning();
  const edgeDriver = ensureEdgeDriver();
  const workspace = createWorkspace(captures);
  let failure: unknown = null;
  for (let attempt = 1; attempt <= START_ATTEMPTS; attempt++) {
    try {
      return await startOnce(workspace, edgeDriver, attempt);
    } catch (e) {
      failure = e;
      // The failed attempt's processes are gone (`startOnce` kills them); start over.
      console.error(`E2E: session start ${attempt}/${START_ATTEMPTS} failed: ${String(e)}`);
    }
  }
  if (process.env.SNIPT_E2E_KEEP !== "1") {
    rmSync(workspace.root, { recursive: true, force: true, maxRetries: 5, retryDelay: 500 });
  }
  throw failure;
}
