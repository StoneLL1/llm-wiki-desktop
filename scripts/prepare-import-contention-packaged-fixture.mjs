import process from "node:process";
import { performance } from "node:perf_hooks";
import { mkdir, writeFile } from "node:fs/promises";
import path from "node:path";

const args = new Map();
for (let index = 2; index < process.argv.length; index += 2) {
  args.set(process.argv[index], process.argv[index + 1]);
}

const delay = (milliseconds) => new Promise((resolve) => setTimeout(resolve, milliseconds));

class CdpClient {
  constructor(socket) {
    this.socket = socket;
    this.nextId = 0;
    this.pending = new Map();
    socket.addEventListener("message", (event) => {
      const message = JSON.parse(event.data);
      const waiter = this.pending.get(message.id);
      if (!waiter) return;
      this.pending.delete(message.id);
      clearTimeout(waiter.timer);
      if (message.error) waiter.reject(new Error(message.error.message));
      else waiter.resolve(message.result);
    });
  }

  static async connect(endpoint) {
    const deadline = performance.now() + 30_000;
    while (performance.now() < deadline) {
      try {
        const targets = await fetch(`${endpoint}/json`, {
          signal: AbortSignal.timeout(2_000),
        }).then((response) => response.json());
        const target = targets.find((candidate) => candidate.type === "page" && candidate.url !== "about:blank");
        if (target) {
          const socket = new WebSocket(target.webSocketDebuggerUrl);
          await new Promise((resolve, reject) => {
            const timer = setTimeout(() => reject(new Error("CDP socket open timed out.")), 5_000);
            socket.addEventListener("open", () => {
              clearTimeout(timer);
              resolve();
            }, { once: true });
            socket.addEventListener("error", reject, { once: true });
          });
          return new CdpClient(socket);
        }
      } catch {
        // The packaged WebView can advertise its target before navigation.
      }
      await delay(50);
    }
    throw new Error(`No packaged WebView2 page was available at ${endpoint}.`);
  }

  command(method, params = {}, timeoutMs = 20_000) {
    return new Promise((resolve, reject) => {
      const id = ++this.nextId;
      const timer = setTimeout(() => {
        this.pending.delete(id);
        reject(new Error(`${method} timed out.`));
      }, timeoutMs);
      this.pending.set(id, { resolve, reject, timer });
      this.socket.send(JSON.stringify({ id, method, params }));
    });
  }

  async evaluate(expression, timeoutMs = 20_000) {
    const response = await this.command("Runtime.evaluate", {
      expression,
      awaitPromise: true,
      returnByValue: true,
    }, timeoutMs);
    if (response.exceptionDetails) {
      throw new Error(response.exceptionDetails.exception?.description ?? response.exceptionDetails.text);
    }
    return response.result.value;
  }

  close() {
    this.socket.close();
  }
}

async function main() {
  const endpoint = args.get("--endpoint") ?? "http://127.0.0.1:9223";
  const projectParent = args.get("--project-parent");
  const projectName = args.get("--project-name") ?? "batch9-packaged-fixture";
  const itemCount = Number(args.get("--items") ?? "10000");
  const startImportBatch = args.get("--start-import-batch") !== "no";
  const syntheticSourceRoot = args.get("--synthetic-source-root");
  const syntheticFileBytes = Number(args.get("--synthetic-file-bytes") ?? "8192");
  if (!projectParent) throw new Error("--project-parent is required.");
  if (!Number.isInteger(itemCount) || itemCount < 1) {
    throw new Error("--items must be a positive integer.");
  }
  if (!Number.isInteger(syntheticFileBytes) || syntheticFileBytes < 128) {
    throw new Error("--synthetic-file-bytes must be an integer of at least 128.");
  }
  if (syntheticSourceRoot) {
    await mkdir(syntheticSourceRoot, { recursive: true });
    const body = `# Batch 9 synthetic source\n\n${"bounded fixture text ".repeat(Math.ceil(syntheticFileBytes / 21))}`
      .slice(0, syntheticFileBytes);
    for (let offset = 0; offset < itemCount; offset += 250) {
      await Promise.all(Array.from(
        { length: Math.min(250, itemCount - offset) },
        (_, index) => writeFile(
          path.join(syntheticSourceRoot, `fixture-${String(offset + index).padStart(5, "0")}.md`),
          body,
          "utf8",
        ),
      ));
    }
  }

  const client = await CdpClient.connect(endpoint);
  try {
    const created = await client.evaluate(`(async () => {
      localStorage.setItem("llm-wiki-desktop.lastProjectParent", ${JSON.stringify(projectParent)});
      const openButton = document.querySelector(".no-project-action")
        ?? Array.from(document.querySelectorAll("button")).find((button) => button.querySelector("svg"));
      if (!openButton) throw new Error("New-project action is not available.");
      openButton.click();
      const deadline = performance.now() + 10000;
      let input;
      while (!(input = document.querySelector('form [aria-labelledby], form input:not([readonly])'))) {
        if (performance.now() > deadline) throw new Error("New-project dialog timed out.");
        await new Promise(requestAnimationFrame);
      }
      const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value").set;
      setter.call(input, ${JSON.stringify(projectName)});
      input.dispatchEvent(new Event("input", { bubbles: true }));
      input.dispatchEvent(new Event("change", { bubbles: true }));
      await new Promise(requestAnimationFrame);
      input.closest("form").requestSubmit();
      while (!document.querySelector(".import-v2-queue")) {
        if (performance.now() > deadline + 30000) throw new Error("Project creation timed out.");
        await new Promise((resolve) => setTimeout(resolve, 50));
      }
      return true;
    })()`, 45_000);
    if (!created) throw new Error("Project creation did not complete.");

    return await client.evaluate(`(async () => {
      const invoke = window.__TAURI_INTERNALS__?.invoke;
      if (!invoke) throw new Error("Packaged Tauri invoke bridge is unavailable.");
      const recent = await invoke("list_recent_projects");
      const project = recent.find((entry) => entry.name === ${JSON.stringify(projectName)});
      if (!project) throw new Error("Prepared project was not found in recent projects.");
      const requestBase = { projectId: project.projectId, projectRootPath: project.rootPath };
      const session = await invoke("create_import_session_v2", {
        request: { ...requestBase, resourceMode: "performance" },
      });
      const syntheticSourceRoot = ${JSON.stringify(syntheticSourceRoot?.replaceAll("\\", "/") ?? null)};
      const inputs = Array.from({ length: ${itemCount} }, (_, index) => ({
        kind: "file",
        displayName: "fixture-" + String(index).padStart(5, "0") + ".md",
        locator: syntheticSourceRoot
          ? syntheticSourceRoot + "/fixture-" + String(index).padStart(5, "0") + ".md"
          : "C:\\\\batch9-missing\\\\fixture-" + String(index).padStart(5, "0") + ".md",
        normalizedLocator: null,
        sourceIdentity: null,
        mediaSaveMode: "extract_only",
      }));
      const populated = await invoke("add_import_items_v2", {
        request: { ...requestBase, sessionId: session.sessionId, inputs },
      });
      const task = ${startImportBatch} ? await invoke("start_import_batch_v2", {
          request: {
            ...requestBase,
            sessionId: session.sessionId,
            itemIds: populated.items.map((item) => item.itemId),
            recoveryAction: null,
          },
        }) : null;
      const wiki = document.querySelector('button[data-app-view="wiki"]');
      const importButton = document.querySelector('button[data-app-view="import"]');
      wiki?.click();
      await new Promise((resolve) => setTimeout(resolve, 100));
      importButton?.click();
      const deadline = performance.now() + 30000;
      while (!document.querySelector(".import-v2-queue")) {
        if (performance.now() > deadline) throw new Error("Import Queue remount timed out.");
        await new Promise(requestAnimationFrame);
      }
      return {
        schemaVersion: 1,
        status: "ready",
        fixtureItems: populated.items.length,
        syntheticSourcesCreated: Boolean(syntheticSourceRoot),
        syntheticFileBytes: syntheticSourceRoot ? ${syntheticFileBytes} : null,
        operationTaskIdPresent: Boolean(task?.id),
        operationTaskStatus: task?.status ?? null,
        privacy: "Only fixture counts and task-id presence are emitted.",
      };
    })()`, 600_000);
  } finally {
    client.close();
  }
}

let result;
try {
  result = await main();
} catch (error) {
  process.exitCode = 1;
  result = { schemaVersion: 1, status: "failed", error: error instanceof Error ? error.message : String(error) };
}
process.stdout.write(`${JSON.stringify(result, null, 2)}\n`);
