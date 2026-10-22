import { chromium } from "../web/node_modules/playwright-core/index.mjs";
import assert from "node:assert/strict";
import { readFile, writeFile, mkdir } from "node:fs/promises";
import { randomUUID } from "node:crypto";
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
  ".tmp/library-recovery-" + Date.now();
await mkdir(output, { recursive: true, mode: 0o700 });
const manifest = JSON.parse(
  await readFile(process.argv[3] ?? ".tmp/library-check/manifest.json", "utf8"),
);
const api = base + "/api/identity/workspaces/" + manifest.workspace,
  route = base + "/w/" + manifest.workspace + "/?view=library";
const headers = { Origin: base, "X-Ayati-Request": "identity" };
const checks = [],
  errors = [];
let page;
const browser = await chromium.launch({
  headless: true,
  executablePath: "/usr/bin/chromium",
  args: ["--no-sandbox"],
});
const pass = (name) => {
  checks.push({ name, passed: true });
  console.log("PASS", name);
};
const poll = async (fn) => {
  for (let i = 0; i < 60; i++) {
    if (await fn()) return;
    await new Promise((r) => setTimeout(r, 100));
  }
  throw Error("Condition did not settle");
};
try {
  const context = await browser.newContext({
    viewport: { width: 1440, height: 960 },
  });
  await context.request.post(
    base + "/api/identity/simulation/employees/0/login",
    { headers, data: {} },
  );
  page = await context.newPage();
  page.on("pageerror", (e) => errors.push(String(e)));
  await page.goto(route + "&item=" + manifest.page);
  await page
    .getByRole("textbox", { name: "Page content", exact: true })
    .waitFor();
  const beforeText = (await page.locator(".library-prose").innerText()).trim();
  const before = await (
    await context.request.get(api + "/library/" + manifest.page)
  ).json();
  await page
    .getByRole("textbox", { name: "Page content", exact: true })
    .fill(
      "Updated onboarding plan. Verify delivery dates and the customer scope.",
    );
  assert.equal(
    await page
      .getByRole("button", { name: "Ask an agent", exact: true })
      .isDisabled(),
    true,
  );
  await page.getByRole("button", { name: "Save", exact: true }).click();
  await poll(
    async () =>
      (
        await (
          await context.request.get(api + "/library/" + manifest.page)
        ).json()
      ).revision > before.revision,
  );
  await poll(
    async () =>
      !(await page
        .getByRole("button", { name: "Ask an agent", exact: true })
        .isDisabled()),
  );
  await page.getByRole("button", { name: "Ask an agent", exact: true }).click();
  const justSaved = await (
    await context.request.get(api + "/library/" + manifest.page)
  ).json();
  assert.ok(
    (
      await page.getByRole("dialog", { name: "Ask an agent" }).innerText()
    ).includes(`revision ${justSaved.revision}`),
  );
  await page.keyboard.press("Escape");
  pass(
    "Agent handoff waits for saving and immediately uses the acknowledged revision",
  );
  await page.getByRole("button", { name: "History", exact: true }).click();
  await page.getByRole("dialog", { name: "Version history" }).waitFor();
  await page
    .locator(".library-version")
    .filter({ hasText: "Revision " + before.revision + " ·" })
    .getByRole("button", { name: "Restore" })
    .click();
  await page.getByRole("dialog").waitFor({ state: "hidden" });
  await poll(
    async () =>
      (await page.locator(".library-prose").innerText()).trim() === beforeText,
  );
  pass("Version restore creates a new revision and reloads the saved content");
  // A server acknowledgement lost in transit must be reconciled, never duplicated.
  let lost = true;
  const sent = [];
  await page.route("**/api/identity/workspaces/*/library", async (r) => {
    if (r.request().method() === "POST") {
      sent.push(r.request().postDataJSON());
      if (lost) {
        lost = false;
        await r.fetch();
        await r.abort("failed");
        return;
      }
    }
    await r.continue();
  });
  const current = await (
    await context.request.get(api + "/library/" + manifest.page)
  ).json();
  await page
    .getByRole("textbox", { name: "Page content", exact: true })
    .fill("Acknowledge this revision exactly once.");
  await page.getByRole("button", { name: "Save", exact: true }).click();
  await page.getByText(/Failed to fetch|Connection|network/i).waitFor();
  await page.getByRole("button", { name: "Save", exact: true }).click();
  await page.getByText(/All changes saved/).waitFor();
  const acknowledged = await (
    await context.request.get(api + "/library/" + manifest.page)
  ).json();
  assert.equal(acknowledged.revision, current.revision + 1);
  assert.equal(sent[0].request, sent[1].request);
  pass(
    "Lost save acknowledgement retries the same request and does not duplicate a revision",
  );
  await page.unroute("**/api/identity/workspaces/*/library");
  // Two sessions edit the same item. A late save must preserve its draft and fail.
  const sim = await (
    await context.request.get(base + "/api/identity/simulation")
  ).json();
  const member = sim.employees.find((e) => e.number === 3).user;
  let latest = await (
    await context.request.get(api + "/library/" + manifest.page)
  ).json();
  assert.ok(
    (
      await context.request.post(api + "/library/" + manifest.page + "/share", {
        headers,
        data: {
          request: randomUUID(),
          revision: latest.access_revision,
          visibility: "selected",
          people: [{ user: member.id, access: "edit" }],
          agents: [manifest.agent],
        },
      })
    ).ok(),
  );
  const other = await browser.newContext();
  await other.request.post(
    base + "/api/identity/simulation/employees/3/login",
    { headers, data: {} },
  );
  const p = await other.newPage();
  p.on("pageerror", (e) => errors.push(String(e)));
  await p.goto(route + "&item=" + manifest.page);
  await p.getByRole("textbox", { name: "Page content", exact: true }).waitFor();
  latest = await (
    await context.request.get(api + "/library/" + manifest.page)
  ).json();
  assert.ok(
    (
      await context.request.post(api + "/library", {
        headers,
        data: {
          request: randomUUID(),
          id: manifest.page,
          revision: latest.revision,
          title: latest.title,
          document: {
            type: "doc",
            content: [
              {
                type: "paragraph",
                content: [
                  { type: "text", text: "Canonical concurrent revision." },
                ],
              },
            ],
          },
        },
      })
    ).ok(),
  );
  await p
    .getByRole("textbox", { name: "Page content", exact: true })
    .fill("Keep my conflicting draft.");
  await p.getByRole("button", { name: "Save", exact: true }).click();
  await p.getByText(/Your draft is kept in this browser/).waitFor();
  assert.match(
    await p.locator(".library-prose").innerText(),
    /conflicting draft/,
  );
  assert.match(
    (
      await (
        await context.request.get(api + "/library/" + manifest.page)
      ).json()
    ).document.content[0].content[0].text,
    /Canonical/,
  );
  pass(
    "Concurrent editor receives a conflict without losing their draft or overwriting the saved page",
  );
  await other.close();
  // Failed transfer survives feature navigation and retries exact immutable bytes.
  await page.goto(route);
  let interrupted = true;
  await page.route("**/library-uploads/*/bytes", async (r) => {
    if (interrupted) {
      interrupted = false;
      await r.abort("failed");
    } else await r.continue();
  });
  const filename = "recover-" + randomUUID().slice(0, 6) + ".txt";
  const file = {
    name: filename,
    mimeType: "text/plain",
    buffer: Buffer.from("Recover this exact file after a network failure."),
  };
  await page
    .locator('input[aria-label="Upload Library files"]')
    .setInputFiles(file);
  await page
    .getByText("Connection interrupted. Retry or reselect the same file.", {
      exact: true,
    })
    .waitFor();
  await page.getByRole("link", { name: "Chat", exact: true }).click();
  assert.equal(
    await page.getByRole("complementary", { name: "File uploads" }).count(),
    1,
  );
  await page.getByRole("link", { name: "Library", exact: true }).click();
  await page.getByRole("button", { name: "Retry", exact: true }).click();
  await page.getByRole("link", { name: filename, exact: true }).waitFor();
  pass(
    "Interrupted upload persists across feature navigation and retries successfully",
  );
  await page.unroute("**/library-uploads/*/bytes");
  // Generate a minimal fixture PDF with one text-bearing page.
  const content =
    "BT /F1 16 Tf 50 740 Td (Library PDF reference: verify the customer scope.) Tj ET";
  const objects = [
    "<< /Type /Catalog /Pages 2 0 R >>",
    "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
    "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>",
    "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>",
    `<< /Length ${Buffer.byteLength(content)} >>\nstream\n${content}\nendstream`,
  ];
  let pdf = "%PDF-1.4\n",
    offsets = [0];
  for (let i = 0; i < objects.length; i++) {
    offsets.push(Buffer.byteLength(pdf));
    pdf += `${i + 1} 0 obj\n${objects[i]}\nendobj\n`;
  }
  const xref = Buffer.byteLength(pdf);
  pdf += `xref\n0 6\n0000000000 65535 f \n${offsets
    .slice(1)
    .map((n) => String(n).padStart(10, "0") + " 00000 n ")
    .join(
      "\n",
    )}\ntrailer\n<< /Size 6 /Root 1 0 R >>\nstartxref\n${xref}\n%%EOF`;
  const pdfname = "reference-" + randomUUID().slice(0, 6) + ".pdf";
  await page.locator('input[aria-label="Upload Library files"]').setInputFiles({
    name: pdfname,
    mimeType: "application/pdf",
    buffer: Buffer.from(pdf),
  });
  await page.getByRole("link", { name: pdfname, exact: true }).waitFor();
  await page.getByRole("link", { name: pdfname, exact: true }).click();
  await page.getByRole("img", { name: /PDF page 1/ }).waitFor();
  await poll(() =>
    page.locator("canvas").evaluate((n) => n.width > 0 && n.height > 0),
  );
  await page.screenshot({ path: output + "/pdf-preview.png" });
  const pdfid = new URL(page.url()).searchParams.get("item");
  const pdfitem = await (
    await context.request.get(api + "/library/" + pdfid)
  ).json();
  assert.equal(pdfitem.readiness, "partial");
  pass(
    "PDF renders through the lazy viewer and bounded sandboxed text extraction completes",
  );
  await page.goto(route + "&item=" + manifest.page);
  await page.getByRole("button", { name: "Share", exact: true }).click();
  await page.getByRole("dialog", { name: "People and agents" }).waitFor();
  await page.keyboard.press("Escape");
  assert.equal(
    await page
      .getByRole("button", { name: "Share", exact: true })
      .evaluate((n) => n === document.activeElement),
    true,
  );
  pass("Escape closes sharing and restores keyboard focus");
  for (const viewport of [
    { width: 320, height: 640 },
    { width: 900, height: 420 },
  ]) {
    await page.setViewportSize(viewport);
    assert.ok(
      await page.evaluate(
        () => document.documentElement.scrollWidth <= innerWidth,
      ),
    );
    await page.screenshot({ path: output + `/editor-${viewport.width}.png` });
  }
  pass("Editor remains contained in narrow and short windows");
  assert.deepEqual(errors, []);
  pass("Recovery journeys have no unhandled browser errors");
} catch (error) {
  console.error(error);
  checks.push({
    name: "Recovery verification",
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
