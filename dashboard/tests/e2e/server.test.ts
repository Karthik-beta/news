// End-to-end: start the real dashboard process (`bun src/index.ts`) on a free port and talk to it over HTTP.
// Loopback only; no Docker and no services. Run with `bun run test:e2e` in dashboard/.
import { afterEach, describe, expect, test } from "bun:test";
import { createServer } from "node:net";
import { networkInterfaces } from "node:os";
import { join } from "node:path";

const cwd = join(import.meta.dir, "..", "..");
const entry = join(cwd, "src", "index.ts");

type Running = { proc: Bun.Subprocess<"ignore", "pipe", "pipe">; port: number; url: string };
const started: Bun.Subprocess[] = [];

afterEach(async () => {
  for (const p of started.splice(0)) {
    p.kill();
    await p.exited;
  }
});

async function freePort(): Promise<number> {
  const server = createServer();
  await new Promise<void>((resolve) => server.listen(0, "127.0.0.1", resolve));
  const { port } = server.address() as { port: number };
  await new Promise<void>((resolve) => server.close(() => resolve()));
  return port;
}

function spawnDashboard(port: string) {
  const proc = Bun.spawn([process.execPath, entry], {
    cwd,
    env: { ...process.env, NEWS_DASHBOARD_PORT: port },
    stdin: "ignore",
    stdout: "pipe",
    stderr: "pipe",
  });
  started.push(proc);
  return proc;
}

/** Read stdout until the "dashboard on ..." line appears (or the process ends). */
async function waitForListening(proc: Bun.Subprocess<"ignore", "pipe", "pipe">, timeoutMs = 10_000): Promise<string> {
  const reader = proc.stdout.getReader();
  const decoder = new TextDecoder();
  let text = "";
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    const next = await Promise.race([reader.read(), Bun.sleep(deadline - Date.now()).then(() => null)]);
    if (!next || next.done) break;
    text += decoder.decode(next.value);
    if (text.includes("\n")) break;
  }
  reader.releaseLock();
  return text;
}

async function startDashboard(): Promise<Running> {
  const port = await freePort();
  const proc = spawnDashboard(String(port));
  const announced = await waitForListening(proc);
  expect(announced, "the dashboard never announced it was listening").toContain("dashboard on");
  return { proc, port, url: `http://127.0.0.1:${port}` };
}

async function failsToConnect(url: string): Promise<boolean> {
  try {
    await fetch(url, { signal: AbortSignal.timeout(2000) });
    return false;
  } catch {
    return true;
  }
}

describe("dashboard server (real process)", () => {
  test("announces its URL and answers GET / with 200 and a plain-text body", async () => {
    const { url } = await startDashboard();
    const res = await fetch(url);
    expect(res.status).toBe(200);
    expect(res.headers.get("content-type")).toBe("text/plain; charset=utf-8");
    expect((await res.text()).length).toBeGreaterThan(0);
  });

  test("announces the port it is really listening on", async () => {
    const port = await freePort();
    const proc = spawnDashboard(String(port));
    expect(await waitForListening(proc)).toContain(`http://127.0.0.1:${port}`);
  });

  test("answers other paths and HEAD without erroring, and repeated requests are consistent", async () => {
    const { url } = await startDashboard();
    const first = await (await fetch(url)).text();
    for (const path of ["/", "/pages", "/a/b/c?x=1", "/%00"]) {
      const res = await fetch(url + path);
      expect(res.status, path).toBeLessThan(500);
    }
    expect((await fetch(url, { method: "HEAD" })).status).toBeLessThan(500);
    expect(await (await fetch(url)).text()).toBe(first);
  });

  test("handles concurrent requests", async () => {
    const { url } = await startDashboard();
    const responses = await Promise.all(Array.from({ length: 50 }, () => fetch(url)));
    expect(responses.every((r) => r.status === 200)).toBe(true);
  });

  const external = Object.values(networkInterfaces())
    .flat()
    .find((i) => i && i.family === "IPv4" && !i.internal)?.address;

  test.skipIf(!external)("listens on localhost only: the machine's external address refuses connections", async () => {
    const { port, url } = await startDashboard();
    expect((await fetch(url)).status).toBe(200);
    expect(await failsToConnect(`http://${external}:${port}`)).toBe(true);
  });

  test("a second instance on a busy port exits with an error instead of hijacking it", async () => {
    const { port, url } = await startDashboard();
    const second = spawnDashboard(String(port));
    const code = await Promise.race([second.exited, Bun.sleep(10_000).then(() => "timeout" as const)]);
    expect(code).not.toBe("timeout");
    expect(code).not.toBe(0);
    expect(await new Response(second.stderr).text()).toMatch(/in use|EADDRINUSE/i);
    // The first instance is unaffected.
    expect((await fetch(url)).status).toBe(200);
  });

  test("stops when killed and releases the port", async () => {
    const { proc, port, url } = await startDashboard();
    proc.kill();
    await proc.exited;
    expect(await failsToConnect(url)).toBe(true);
    // The port can be bound again straight away.
    const again = spawnDashboard(String(port));
    expect(await waitForListening(again)).toContain("dashboard on");
  });

  test("an invalid NEWS_DASHBOARD_PORT makes the process exit non-zero", async () => {
    for (const bad of ["abc", "70000", "-1", "1.5"]) {
      const proc = spawnDashboard(bad);
      const code = await Promise.race([proc.exited, Bun.sleep(10_000).then(() => "timeout" as const)]);
      expect(code, `NEWS_DASHBOARD_PORT=${bad}`).not.toBe("timeout");
      expect(code, `NEWS_DASHBOARD_PORT=${bad}`).not.toBe(0);
    }
  });

  // BUG (reported, not fixed): an empty NEWS_DASHBOARD_PORT (e.g. `NEWS_DASHBOARD_PORT=` in .env) is read as
  // Number("") === 0, so Bun picks a random port and the log line announces `http://127.0.0.1:0`. It should
  // fall back to 4400 or fail. Flip to a plain `test` when it is fixed.
  test.failing("an empty NEWS_DASHBOARD_PORT does not announce port 0", async () => {
    const proc = spawnDashboard("");
    const announced = await waitForListening(proc);
    expect(announced).not.toContain("127.0.0.1:0");
  });
});
