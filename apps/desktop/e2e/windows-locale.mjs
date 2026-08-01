import assert from "node:assert/strict";
import { execFile as execFileCallback, spawn } from "node:child_process";
import { mkdir, open, readFile, rm, stat, writeFile } from "node:fs/promises";
import net from "node:net";
import path from "node:path";
import { clearTimeout, setTimeout } from "node:timers";
import { setTimeout as delay } from "node:timers/promises";
import { promisify } from "node:util";

const execFile = promisify(execFileCallback);
const webdriverEndpoint = "http://127.0.0.1:4444";
const webdriverElementKey = "element-6066-11e4-a52e-4f735466cecf";
const sessionStartAttempts = 3;
const sessionStartRequestTimeoutMs = 30_000;
const proxyTypeProxy = 0x2;
const autoProxyFlags = 0x4 | 0x8;
const repositoryRoot = path.resolve(import.meta.dirname, "../../..");
const proxyStateScript = path.join(import.meta.dirname, "proxy-state.ps1");
const appPath = requiredEnvironment("DADA_E2E_APP");
const appPidPath = requiredEnvironment("DADA_E2E_APP_PID_PATH");
const chatGptPath = requiredEnvironment("DADA_E2E_CHATGPT_PATH");
const localeHome = requiredEnvironment("DADA_E2E_LOCALE_HOME");
const baselineStatePath = requiredEnvironment("DADA_E2E_BASELINE_PROXY_STATE");
const proxyMutationMarkerPath = requiredEnvironment("DADA_E2E_PROXY_MUTATION_MARKER");
const webviewUserDataFolder = requiredEnvironment("DADA_E2E_WEBVIEW2_USER_DATA_FOLDER");
const remoteDebugPort = parseRemoteDebugPort(requiredEnvironment("DADA_E2E_REMOTE_DEBUG_PORT"));
const artifactDirectory = requiredEnvironment("DADA_E2E_ARTIFACT_DIR");
const remoteDebuggerEndpoint = `http://127.0.0.1:${remoteDebugPort}`;

async function run() {
  if (process.platform !== "win32") {
    throw new Error("Windows locale E2E only runs on Windows.");
  }

  await mkdir(artifactDirectory, { recursive: true });
  await rm(localeHome, { recursive: true, force: true });
  await mkdir(localeHome, { recursive: true });
  await rm(webviewUserDataFolder, { recursive: true, force: true });
  await mkdir(webviewUserDataFolder, { recursive: true });

  let desktopApplication;
  let driver;
  try {
    desktopApplication = await startDesktopApplication();
    await waitForRemoteDebugger(desktopApplication);
    driver = await WebDriverClient.connect();
    await prepareProxyBaseline();

    const configureChinese = await driver.waitForElement('[data-testid="configure-chinese"]');
    await driver.waitForEnabled(configureChinese);
    await driver.click(configureChinese);

    const primaryAction = await driver.waitForElement('[data-testid="locale-primary-action"]');
    await driver.waitForEnabled(primaryAction);
    await driver.click(primaryAction);

    for (const step of [1, 2, 3, 4]) {
      await waitForComplete(driver, step);
    }
    const recoveryStep = await driver.element('[data-testid="locale-step-5"]');
    assert.equal((await driver.attribute(recoveryStep, "class"))?.includes("complete"), false);
    await driver.waitUntil(
      async () => (await driver.text(primaryAction)).includes("恢复原网络"),
      "中文配置完成后未出现恢复原网络操作",
      30_000,
    );

    await saveScreenshot(driver, "activation-succeeded.png");
    await assertLocaleFiles();
    const activationProxy = await readProxyState();
    const baselineState = JSON.parse(await readFile(baselineStatePath, "utf8"));
    assert.equal(activationProxy.proxyEnable.exists, true);
    assert.equal(activationProxy.proxyEnable.value, 1);
    assert.equal(activationProxy.proxyServer.exists, true);
    const loopbackEndpoint = parseLoopbackEndpoint(activationProxy.proxyServer.value);
    assert.match(activationProxy.proxyOverride.value, /localhost/i);
    assert.equal(activationProxy.autoConfigUrl.exists, false);
    assert.equal(activationProxy.perConnectionFlags.exists, true);
    assert.equal(activationProxy.perConnectionFlags.value & proxyTypeProxy, proxyTypeProxy);
    assert.equal(activationProxy.perConnectionFlags.value & autoProxyFlags, 0);
    await assertLoopbackProxy(loopbackEndpoint);
    await assertZhCnRenderer();

    const recoveryPath = path.join(localeHome, "recovery.json");
    const recoveryRecord = JSON.parse(await readFile(recoveryPath, "utf8"));
    assert.equal(recoveryRecord.operatingSystem, "windows");
    assert.deepEqual(JSON.parse(recoveryRecord.networkState), baselineState);
    assert.notEqual(baselineState.perConnectionFlags.value & autoProxyFlags, 0);

    await driver.waitForEnabled(primaryAction);
    await driver.click(primaryAction);
    await waitForComplete(driver, 5);
    await driver.waitUntil(
      async () => (await driver.text(primaryAction)).includes("重新设置"),
      "恢复原网络后主操作未回到可重新设置状态",
      30_000,
    );

    await assertFileMissing(recoveryPath);
    assert.deepEqual(await readProxyState(), baselineState);
    await saveScreenshot(driver, "network-restored.png");
  } catch (error) {
    if (driver) {
      await saveScreenshot(driver, "failure.png").catch(() => undefined);
    }
    throw error;
  } finally {
    await driver?.deleteSession().catch(() => undefined);
  }
}

class WebDriverClient {
  constructor(sessionId) {
    this.sessionId = sessionId;
  }

  static async connect() {
    let lastError;
    for (let attempt = 0; attempt < sessionStartAttempts; attempt += 1) {
      try {
        const response = await requestWebDriver(
          "POST",
          "/session",
          {
            capabilities: {
              alwaysMatch: {
                browserName: "webview2",
                "ms:edgeChromium": true,
                "ms:edgeOptions": {
                  debuggerAddress: `127.0.0.1:${remoteDebugPort}`,
                },
              },
            },
          },
          sessionStartRequestTimeoutMs,
        );
        const sessionId = response.value?.sessionId ?? response.sessionId;
        if (typeof sessionId !== "string" || sessionId.length === 0) {
          throw new Error("WebDriver did not return a session ID.");
        }
        return new WebDriverClient(sessionId);
      } catch (error) {
        lastError = error;
        if (!isRetryableSessionStartError(error) || attempt === sessionStartAttempts - 1) {
          break;
        }
        await delay(1_000);
      }
    }
    throw new Error("Unable to start the Tauri WebDriver session.", { cause: lastError });
  }

  async element(selector) {
    const response = await this.request("POST", "/element", {
      using: "css selector",
      value: selector,
    });
    const elementId = response?.[webdriverElementKey] ?? response?.ELEMENT;
    if (typeof elementId !== "string" || elementId.length === 0) {
      throw new Error(`WebDriver did not return an element for ${selector}.`);
    }
    return elementId;
  }

  async waitForElement(selector, timeout = 30_000) {
    let elementId;
    await this.waitUntil(
      async () => {
        elementId = await this.element(selector);
        return true;
      },
      `未找到测试元素：${selector}`,
      timeout,
    );
    return elementId;
  }

  async waitForEnabled(elementId, timeout = 30_000) {
    await this.waitUntil(
      async () => this.request("GET", `/element/${encodeURIComponent(elementId)}/enabled`),
      "操作按钮没有变为可用状态",
      timeout,
    );
  }

  async click(elementId) {
    await this.request("POST", `/element/${encodeURIComponent(elementId)}/click`, {});
  }

  async text(elementId) {
    return this.request("GET", `/element/${encodeURIComponent(elementId)}/text`);
  }

  async attribute(elementId, name) {
    return this.request(
      "GET",
      `/element/${encodeURIComponent(elementId)}/attribute/${encodeURIComponent(name)}`,
    );
  }

  async saveScreenshot(filePath) {
    const image = await this.request("GET", "/screenshot");
    await writeFile(filePath, Buffer.from(image, "base64"));
  }

  async deleteSession() {
    await requestWebDriver("DELETE", `/session/${encodeURIComponent(this.sessionId)}`);
  }

  async waitUntil(predicate, message, timeout) {
    const deadline = Date.now() + timeout;
    let lastError;
    while (Date.now() < deadline) {
      try {
        if (await predicate()) return;
      } catch (error) {
        lastError = error;
      }
      await delay(250);
    }
    throw new Error(message, { cause: lastError });
  }

  async request(method, pathSuffix, payload) {
    const response = await requestWebDriver(
      method,
      `/session/${encodeURIComponent(this.sessionId)}${pathSuffix}`,
      payload,
    );
    return response.value;
  }
}

await run();

async function requestWebDriver(method, pathSuffix, payload, timeoutMs = 30_000) {
  const response = await fetch(`${webdriverEndpoint}${pathSuffix}`, {
    method,
    headers: payload === undefined ? undefined : { "content-type": "application/json" },
    body: payload === undefined ? undefined : JSON.stringify(payload),
    signal: AbortSignal.timeout(timeoutMs),
  });
  const responseText = await response.text();
  let responseBody = {};
  if (responseText) {
    try {
      responseBody = JSON.parse(responseText);
    } catch {
      throw new Error(`WebDriver returned an invalid response (${response.status}).`);
    }
  }
  if (!response.ok || responseBody.value?.error) {
    const detail = responseBody.value?.message ?? responseBody.value?.error ?? responseText;
    throw new Error(`WebDriver request failed (${response.status}): ${detail}`);
  }
  return responseBody;
}

function isRetryableSessionStartError(error) {
  if (!(error instanceof Error)) return false;
  if (error.name === "TimeoutError" || error.name === "TypeError") return true;
  return /WebDriver request failed \(50[234]\)/.test(error.message);
}

async function startDesktopApplication() {
  const stdout = await open(path.join(artifactDirectory, "desktop-app.log"), "w");
  const stderr = await open(path.join(artifactDirectory, "desktop-app-error.log"), "w");
  try {
    const child = spawn(appPath, [], {
      env: process.env,
      stdio: ["ignore", stdout.fd, stderr.fd],
      windowsHide: true,
    });
    let startupError;
    let exitStatus;
    child.once("error", (error) => {
      startupError = error;
    });
    child.once("exit", (code, signal) => {
      exitStatus = { code, signal };
    });
    if (!Number.isSafeInteger(child.pid) || child.pid <= 0) {
      throw new Error("未能启动 E2E 桌面应用。");
    }
    await writeFile(appPidPath, `${child.pid}\n`, { encoding: "utf8" });
    child.unref();

    return {
      failure() {
        if (startupError) {
          return new Error("E2E 桌面应用启动失败。", { cause: startupError });
        }
        if (exitStatus) {
          return new Error(
            `E2E 桌面应用在 WebView2 调试端点就绪前退出（code=${exitStatus.code}, signal=${exitStatus.signal}）。`,
          );
        }
        return undefined;
      },
    };
  } finally {
    await Promise.all([stdout.close(), stderr.close()]);
  }
}

async function waitForRemoteDebugger(application) {
  const deadline = Date.now() + 30_000;
  let lastError;
  while (Date.now() < deadline) {
    const applicationFailure = application.failure();
    if (applicationFailure) {
      throw applicationFailure;
    }

    try {
      const response = await fetch(`${remoteDebuggerEndpoint}/json/version`, {
        signal: AbortSignal.timeout(1_000),
      });
      if (!response.ok) {
        throw new Error(`WebView2 调试端点返回 ${response.status}。`);
      }
      const description = await response.json();
      if (typeof description.webSocketDebuggerUrl !== "string") {
        throw new Error("WebView2 调试端点没有返回 WebSocket 地址。");
      }
      return;
    } catch (error) {
      lastError = error;
      await delay(250);
    }
  }
  throw new Error("WebView2 调试端点未在限定时间内就绪。", { cause: lastError });
}

async function prepareProxyBaseline() {
  // Do not mutate the runner until EdgeDriver has created a real session. If
  // WebView2 cannot start, cleanup sees no marker and leaves the runner alone.
  await writeFile(proxyMutationMarkerPath, "", { encoding: "utf8" });
  await runProxyStateAction("set-baseline");
  await runProxyStateAction("save", baselineStatePath);
}

async function runProxyStateAction(action, statePath) {
  const args = [
    "-NoProfile",
    "-NonInteractive",
    "-ExecutionPolicy",
    "Bypass",
    "-File",
    proxyStateScript,
    "-Action",
    action,
  ];
  if (statePath) {
    args.push("-StatePath", statePath);
  }
  await execFile("powershell.exe", args, {
    cwd: repositoryRoot,
    windowsHide: true,
  });
}

function requiredEnvironment(name) {
  const value = process.env[name];
  if (!value) {
    throw new Error(`${name} is required.`);
  }
  return value;
}

function parseRemoteDebugPort(value) {
  if (!/^[1-9]\d{0,4}$/.test(value)) {
    throw new Error("DADA_E2E_REMOTE_DEBUG_PORT 必须是非零端口号。");
  }
  const port = Number(value);
  if (port > 65_535) {
    throw new Error("DADA_E2E_REMOTE_DEBUG_PORT 超出端口范围。");
  }
  return port;
}

async function waitForComplete(driver, step) {
  const element = await driver.waitForElement(`[data-testid="locale-step-${step}"]`);
  await driver.waitUntil(
    async () => (await driver.attribute(element, "class"))?.includes("complete"),
    `步骤 ${step} 未完成`,
    45_000,
  );
}

async function saveScreenshot(driver, filename) {
  await driver.saveScreenshot(path.join(artifactDirectory, filename));
}

async function assertLocaleFiles() {
  const config = await readFile(path.join(localeHome, "config.toml"), "utf8");
  assert.match(config, /\[desktop\][\s\S]*localeOverride\s*=\s*"zh-CN"/);

  const globalState = JSON.parse(
    await readFile(path.join(localeHome, ".codex-global-state.json"), "utf8"),
  );
  assert.equal(globalState.localeOverride, "zh-CN");
}

function parseLoopbackEndpoint(proxyServer) {
  const matched = /^127\.0\.0\.1:(\d+)$/.exec(proxyServer);
  assert.ok(matched, `系统代理未指向回环地址：${proxyServer}`);
  return { host: "127.0.0.1", port: Number(matched[1]) };
}

async function assertLoopbackProxy(endpoint) {
  await new Promise((resolve, reject) => {
    const socket = net.createConnection(endpoint);
    let response = "";
    let settled = false;
    const finish = (callback) => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      socket.destroy();
      callback();
    };
    const timer = setTimeout(() => {
      finish(() => reject(new Error("本地代理监听超时")));
    }, 5_000);
    socket.once("connect", () => {
      socket.write("CONNECT e2e.invalid:443 HTTP/1.1\r\nHost: e2e.invalid:443\r\n\r\n");
    });
    socket.on("data", (chunk) => {
      response += chunk.toString("utf8");
      if (response.includes("\r\n\r\n")) {
        try {
          assert.match(response, /^HTTP\/1\.1 502 Bad Gateway\r\n/);
        } catch (error) {
          finish(() => reject(error));
          return;
        }
        finish(resolve);
      }
    });
    socket.once("error", (error) => {
      finish(() => reject(error));
    });
  });
}

async function assertZhCnRenderer() {
  const { stdout } = await execFile(
    "powershell.exe",
    [
      "-NoProfile",
      "-NonInteractive",
      "-ExecutionPolicy",
      "Bypass",
      "-Command",
      "$renderer = Get-CimInstance Win32_Process | Where-Object { $_.Name -eq 'ChatGPT.exe' -and $_.ExecutablePath -and $_.ExecutablePath -ieq $env:DADA_E2E_CHATGPT_PATH -and $_.CommandLine -and $_.CommandLine.Contains('--type=renderer') -and $_.CommandLine.Contains('--lang=zh-CN') } | Select-Object -First 1; if ($null -ne $renderer) { $renderer.CommandLine }",
    ],
    {
      windowsHide: true,
      env: { ...process.env, DADA_E2E_CHATGPT_PATH: chatGptPath },
    },
  );
  assert.match(stdout, /--type=renderer/);
  assert.match(stdout, /--lang=zh-CN/);
}

async function readProxyState() {
  const { stdout } = await execFile(
    "powershell.exe",
    [
      "-NoProfile",
      "-NonInteractive",
      "-ExecutionPolicy",
      "Bypass",
      "-File",
      proxyStateScript,
      "-Action",
      "read",
    ],
    { cwd: repositoryRoot, windowsHide: true },
  );
  return JSON.parse(stdout.trim());
}

async function assertFileMissing(filePath) {
  try {
    await stat(filePath);
  } catch (error) {
    if (error && typeof error === "object" && error.code === "ENOENT") {
      return;
    }
    throw error;
  }
  throw new Error(`恢复记录仍然存在：${filePath}`);
}
