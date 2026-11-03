// The everyday path through the real app, on a fresh data folder (see e2e/run.mjs).
// One file, run in order: later steps rely on what earlier ones created, and the last step
// restarts Pluto to prove it was all saved.
import { after, before, describe, it } from "node:test";
import assert from "node:assert/strict";
import { Builder, By, Key, until } from "selenium-webdriver";

const WAIT = 15_000;
let driver;

async function start() {
  // A restart right after a quit can race the single-instance guard (the old process is still
  // closing), so try a few times.
  for (let attempt = 1; ; attempt++) {
    try {
      await open();
      return;
    } catch (e) {
      await driver?.quit().catch(() => {});
      driver = undefined;
      if (attempt === 3) throw e;
      await new Promise((r) => setTimeout(r, 2000));
    }
  }
}

async function open() {
  driver = await new Builder()
    .usingServer("http://127.0.0.1:4444/")
    // `webviewOptions` tells Microsoft's Edge driver (Windows) that this is a WebView2 app, so
    // it passes its debugging flags to the web view instead of to Pluto's own command line.
    // Other systems ignore it.
    .withCapabilities({ browserName: "wry", "tauri:options": { application: process.env.PLUTO_BINARY, webviewOptions: {} } })
    .build();
  // Pluto has two windows (the dashboard and the pet). Work in the dashboard.
  const deadline = Date.now() + WAIT;
  while (Date.now() < deadline) {
    for (const handle of await driver.getAllWindowHandles()) {
      await driver.switchTo().window(handle);
      if ((await driver.getTitle()) === "Pluto") return;
    }
    await driver.sleep(250);
  }
  throw new Error("The Pluto dashboard window never appeared");
}

const q = (s) => `"${s}"`;
const find = (xpath) => driver.wait(until.elementLocated(By.xpath(xpath)), WAIT);
const visible = async (xpath) => driver.wait(until.elementIsVisible(await find(xpath)), WAIT);
const byLabel = (label) => visible(`//*[@aria-label=${q(label)}]`);
// The innermost element whose whole text contains `t` (text can be split across nodes).
const text = (t) => visible(`//*[contains(normalize-space(.), ${q(t)}) and not(*[contains(normalize-space(.), ${q(t)})])]`);
/** Click what `xpath` finds, finding it again if the page re-rendered it in between. */
async function click(xpath) {
  for (let attempt = 1; ; attempt++) {
    try {
      await (await visible(xpath)).click();
      return;
    } catch (e) {
      if (attempt === 5 || e?.name !== "StaleElementReferenceError") throw e;
      await driver.sleep(200);
    }
  }
}
const xButton = (t) => `//button[normalize-space(.)=${q(t)} or @aria-label=${q(t)}]`;
const xSidebar = (name) => `//aside//button[starts-with(normalize-space(.), ${q(name)})]`;
const gone = async (xpath) => driver.wait(async () => (await driver.findElements(By.xpath(xpath))).length === 0, WAIT);

before(start);
after(async () => { await driver?.quit(); });

describe("Pluto, end to end", () => {
  it("welcomes a fresh install, asks who is using it, then opens on Today", async () => {
    await visible(`//h1[normalize-space(.)="Welcome to Pluto"]`);
    await click(xButton("Start"));
    // Since 0.5.2 a new install asks one question (FR-83) and then starts the tour on Today. The
    // test skips both, as a person in a hurry would; its later steps need the page free.
    await visible(`//h1[normalize-space(.)="What describes you best?"]`);
    await click(xButton("Skip"));
    await click(xButton("Skip tour"));
    await gone(`//*[@role="dialog" and @aria-label="Pluto tour"]`);
    await byLabel("Today’s agenda");
  });

  it("adds a task", async () => {
    await click(xSidebar("Tasks"));
    const input = await byLabel("New task");
    await input.sendKeys("E2E: write the report", Key.ENTER);
    await text("E2E: write the report");
  });

  it("writes a note that saves itself", async () => {
    await click(xSidebar("Notes"));
    await click(`//button[starts-with(normalize-space(.), "New note")]`);
    await (await byLabel("Title")).sendKeys("E2E note");
    await (await byLabel("Note")).sendKeys("Written by the end-to-end test.");
    await driver.sleep(1200); // autosave waits 500 ms after the last key
    await driver.actions().sendKeys(Key.ESCAPE).perform();
    await text("E2E note");
  });

  it("logs a meeting and turns an action item into a task", async () => {
    await click(xSidebar("Meetings"));
    await click(`//button[starts-with(normalize-space(.), "New meeting")]`);
    await (await byLabel("Meeting title")).sendKeys("E2E sync");
    await click(xButton("Add action item"));
    await (await byLabel("Action item 1")).sendKeys("Send the recap");
    await text("Saved");
    await click(xButton("Convert to task"));
    await text("Task · Planned");
    await click(xButton("Back to meetings"));
    await byLabel("Open meeting “E2E sync”");
  });

  it("keeps everything after a restart, and does not welcome again", async () => {
    await driver.quit();
    driver = undefined;
    await new Promise((r) => setTimeout(r, 2000)); // let the old process finish closing
    await start();
    // Pluto reopens on the last page used; any page means it got past the welcome screen.
    await visible(xSidebar("Tasks"));
    await gone(`//h1[normalize-space(.)="Welcome to Pluto"]`);
    await click(xSidebar("Tasks"));
    await click(xButton("All"));
    await text("E2E: write the report");
    await text("Send the recap");
    await click(xSidebar("Notes"));
    await text("E2E note");
    await click(xSidebar("Meetings"));
    await byLabel("Open meeting “E2E sync”");
    assert.ok(true);
  });
});
