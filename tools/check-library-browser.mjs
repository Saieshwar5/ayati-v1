import { chromium } from "../web/node_modules/playwright-core/index.mjs";
import assert from "node:assert/strict";
import { mkdir, writeFile } from "node:fs/promises";
import { randomUUID } from "node:crypto";
// Only the explicit scripted simulation is a valid target; never exercise real accounts.
const target = new URL(process.argv[2] ?? "http://127.0.0.1:8890");
assert.ok(
  target.protocol === "http:" &&
    ["127.0.0.1", "localhost"].includes(target.hostname),
  "Use a loopback scripted simulation",
);
const base = target.origin;
const info = await fetch(base + "/api/identity/simulation");
assert.equal(
  info.status,
  200,
  "Start an isolated simulation before running this check",
);
assert.match((await info.json()).mode, /scripted/);
const output =
  process.env.CUCKOO_LIBRARY_TEST_OUTPUT ??
  ".tmp/library-browser-" + Date.now();
await mkdir(output, { recursive: true, mode: 0o700 });
const browser = await chromium.launch({
  headless: true,
  executablePath: "/usr/bin/chromium",
  args: ["--no-sandbox"],
});
const checks = [],
  errors = [];
let page;
const suffix = randomUUID().slice(0, 6),
  title = "Customer onboarding plan " + suffix,
  agentName = "Library assistant " + suffix,
  fileName = "delivery-procedure-" + suffix + ".txt";
const pass = (name) => {
  checks.push({ name, passed: true });
  console.log("PASS", name);
};
const headers = { Origin: base, "X-Ayati-Request": "identity" };
let context;
try {
  context = await browser.newContext({
    viewport: { width: 1440, height: 960 },
  });
  const login = await context.request.post(
    base + "/api/identity/simulation/employees/0/login",
    { headers, data: {} },
  );
  assert.equal(login.status(), 200);
  const { workspace } = await login.json();
  const api = `${base}/api/identity/workspaces/${workspace}`;
  const route = `${base}/w/${workspace}/?view=library`;
  const agent = randomUUID();
  const agentRes = await context.request.post(api + "/agent-profiles", {
    headers,
    data: {
      id: agent,
      name: agentName,
      role: "Answer questions using the selected Library evidence.",
      model: "simulation",
      scope: "workspace",
      revision: 0,
      settings: {
        instructions: "",
        provider: "",
        escalation: "",
        avatar: "spark",
        memory: false,
        browser: false,
        history: "all",
        paused: false,
      },
    },
  });
  assert.equal(agentRes.status(), 200, await agentRes.text());
  page = await context.newPage();
  page.on("pageerror", (e) => errors.push(String(e)));
  await page.goto(route);
  await page.getByRole("heading", { name: "Library", exact: true }).waitFor();
  await page
    .getByRole("button", { name: "＋ New page", exact: true })
    .waitFor();
  await page.screenshot({ path: output + "/empty.png" });
  await page.getByRole("button", { name: "＋ New page", exact: true }).click();
  await page
    .getByRole("textbox", { name: "Page title", exact: true })
    .waitFor();
  await page
    .getByRole("textbox", { name: "Page title", exact: true })
    .fill(title);
  await page
    .getByRole("textbox", { name: "Page content", exact: true })
    .fill(
      "Welcome new customers. Confirm the approved project scope before scheduling delivery. Keep the owner informed about exceptions.",
    );
  await page.getByText(/All changes saved/).waitFor();
  await page.waitForTimeout(1300);
  const id = new URL(page.url()).searchParams.get("item");
  let item = await (await context.request.get(api + "/library/" + id)).json();
  assert.equal(item.title, title);
  assert.match(
    item.document.content[0].content[0].text,
    /Confirm the approved/,
  );
  pass("Create a native page and autosave title/content");
  await page.getByRole("button", { name: "Bold", exact: true }).click();
  await page.screenshot({ path: output + "/editor-light.png" });
  await page.getByRole("button", { name: "Share", exact: true }).click();
  await page.getByRole("dialog", { name: "People and agents" }).waitFor();
  await page
    .getByLabel("People access", { exact: true })
    .selectOption("selected");
  await page
    .getByLabel("Access for Arjun Patel", { exact: true })
    .selectOption("view");
  await page.getByLabel(agentName, { exact: false }).check();
  await page.screenshot({ path: output + "/share.png" });
  await page.getByRole("button", { name: "Save changes", exact: true }).click();
  await page.getByRole("dialog").waitFor({ state: "hidden" });
  pass("Share with an existing User and grant separate agent reading");
  const member = await browser.newContext({
    viewport: { width: 1280, height: 800 },
  });
  await member.request.post(
    base + "/api/identity/simulation/employees/3/login",
    { headers, data: {} },
  );
  const m = await member.newPage();
  m.on("pageerror", (e) => errors.push(String(e)));
  await m.goto(route + "&item=" + id);
  await m.getByRole("textbox", { name: "Page title" }).waitFor();
  assert.equal(
    await m.getByRole("button", { name: "Share", exact: true }).count(),
    0,
  );
  assert.equal(
    await m
      .getByRole("textbox", { name: "Page title" })
      .getAttribute("readonly"),
    "",
  );
  assert.match(
    await m.locator(".library-prose").innerText(),
    /Confirm the approved/,
  );
  pass("Shared User can read, with no sharing or editing controls");
  await page.getByRole("button", { name: "Ask an agent", exact: true }).click();
  await page.getByLabel("Agent", { exact: true }).selectOption(agent);
  await page
    .getByLabel("Your question", { exact: true })
    .fill("Summarize the onboarding requirements.");
  await page
    .getByRole("button", { name: "Continue in Chat", exact: true })
    .click();
  await page.waitForURL(/view=chat&conversation=/);
  await page.getByRole("log", { name: "Messages", exact: true }).waitFor();
  await page
    .getByText("Library sources supplied to this request (limited excerpts):", {
      exact: false,
    })
    .waitFor();
  pass(
    "Ask creates a private Chat with a source citation through the durable queue",
  );
  const chat = new URL(page.url()).searchParams.get("conversation");
  assert.notEqual(
    (await member.request.get(api + `/chats/${chat}/messages`)).status(),
    200,
  );
  pass("Another workspace member cannot open the source-based private Chat");
  await page.goto(route + "&item=" + id);
  await page.getByRole("button", { name: "Share", exact: true }).click();
  await page
    .getByLabel("People access", { exact: true })
    .selectOption("private");
  await page.getByRole("button", { name: "Save changes", exact: true }).click();
  await page.getByRole("dialog").waitFor({ state: "hidden" });
  await m.reload();
  await m.getByRole("alert").waitFor();
  assert.equal(await m.locator(".library-prose").count(), 0);
  assert.equal(
    (await member.request.get(api + "/library/" + id)).status(),
    404,
  );
  pass("Revoking sharing removes content and blocks direct item reads");
  await member.close();
  await page.goto(route);
  await page.getByRole("link", { name: title, exact: true }).waitFor();
  await page.screenshot({ path: output + "/list-light.png" });
  await page.emulateMedia({ colorScheme: "dark" });
  await page.screenshot({ path: output + "/list-dark.png" });
  pass("List renders in light and dark themes");
  for (const viewport of [
    { width: 390, height: 844 },
    { width: 320, height: 640 },
    { width: 900, height: 420 },
  ]) {
    await page.setViewportSize(viewport);
    assert.ok(
      await page.evaluate(
        () => document.documentElement.scrollWidth <= innerWidth,
      ),
      JSON.stringify(viewport),
    );
    await page.screenshot({ path: output + `/list-${viewport.width}.png` });
  }
  pass("List contains within 320px, 390px and short windows");
  await page.setViewportSize({ width: 1440, height: 960 });
  await page.emulateMedia({ colorScheme: "light" });
  await page
    .locator('input[aria-label="Upload Library files"]')
    .setInputFiles({
      name: fileName,
      mimeType: "text/plain",
      buffer: Buffer.from(
        "Check the order. Confirm stock. Ask the owner before changing an agreed delivery date.",
      ),
    });
  await page.getByRole("link", { name: fileName, exact: true }).waitFor();
  pass("Upload a reference file with immutable bytes and queue completion");
  await page.getByRole("link", { name: fileName, exact: true }).click();
  await page.getByRole("heading", { name: fileName }).waitFor();
  await page.screenshot({ path: output + "/file.png" });
  const fid = new URL(page.url()).searchParams.get("item");
  const file = await context.request.get(api + `/library/${fid}/download`);
  assert.equal(
    await file.text(),
    "Check the order. Confirm stock. Ask the owner before changing an agreed delivery date.",
  );
  assert.match(file.headers()["cache-control"], /no-store/);
  const range = await context.request.get(api + `/library/${fid}/download`, {
    headers: { Range: "bytes=0-4" },
  });
  assert.equal(range.status(), 206);
  assert.equal(await range.text(), "Check");
  pass(
    "Authorized original download and range requests return the expected bytes",
  );
  await writeFile(
    output + "/manifest.json",
    JSON.stringify({ workspace, page: id, file: fid, agent, chat }, null, 2),
  );
  assert.deepEqual(errors, []);
  pass("No unhandled browser errors");
} catch (error) {
  console.error(error);
  checks.push({
    name: "Browser verification",
    passed: false,
    error: String(error),
  });
  process.exitCode = 1;
  if (page)
    await page.screenshot({ path: output + "/failure.png" }).catch(() => {});
} finally {
  await writeFile(
    output + "/results.json",
    JSON.stringify({ checks, errors }, null, 2),
  );
  await browser.close();
}
