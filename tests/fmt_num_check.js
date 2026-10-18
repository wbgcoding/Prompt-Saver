// Numbers a person reads are written the way the UI language writes them
// (ui/floatShared.js: fmtNum, fmtBytes, fmtSizeCompact): a decimal comma in German
// and Polish, a point in English, no change to the tiers and units.
// Also: every fmtBytes call in ui/ names the language of its own window.
// Run:  node tests/fmt_num_check.js      Exit: 0 = clean, 1 = problems.
const fs = require("fs");
const path = require("path");
const { pathToFileURL } = require("url");

const uiDir = path.join(__dirname, "..", "ui");
const problems = [];
const expect = (label, got, want) => {
  if (got !== want) problems.push(`${label}: got ${JSON.stringify(got)}, want ${JSON.stringify(want)}`);
};

async function main() {
  globalThis.document = globalThis.document || { addEventListener() {}, removeEventListener() {} };
  const { fmtNum, fmtBytes, fmtSizeCompact, fmtUnit } = await import(pathToFileURL(path.join(uiDir, "floatShared.js")).href);

  expect("fmtBytes 600000000 de", fmtBytes(600000000, "de"), "572,2 MB");
  expect("fmtBytes 600000000 en", fmtBytes(600000000, "en"), "572.2 MB");
  expect("fmtBytes without a language is English", fmtBytes(600000000), "572.2 MB");
  expect("fmtBytes bytes tier", fmtBytes(512, "de"), "512 B");
  expect("fmtBytes KB tier de", fmtBytes(1536, "de"), "1,5 KB");
  expect("fmtBytes KB tier en", fmtBytes(1536, "en"), "1.5 KB");
  expect("fmtBytes whole MB keeps one decimal", fmtBytes(2 * 1024 * 1024, "de"), "2,0 MB");
  expect("fmtBytes pl", fmtBytes(600000000, "pl"), "572,2 MB");
  expect("fmtBytes ja", fmtBytes(600000000, "ja"), "572.2 MB");

  expect("fmtSizeCompact KB de", fmtSizeCompact(300 * 1024, "de"), "300 KB");
  expect("fmtSizeCompact MB de", fmtSizeCompact(5 * 1024 * 1024 + 512 * 1024, "de"), "5,5 MB");
  expect("fmtSizeCompact MB en", fmtSizeCompact(5 * 1024 * 1024 + 512 * 1024, "en"), "5.5 MB");
  expect("fmtSizeCompact empty", fmtSizeCompact(0, "de"), "");

  expect("fmtNum de 1 digit", fmtNum(1.25, 1, "de"), "1,3");
  expect("fmtNum en 2 digits", fmtNum(3, 2, "en"), "3.00");
  expect("fmtNum 0 digits", fmtNum(2.6, 0, "de"), "3");
  expect("fmtNum unknown language falls back to English", fmtNum(1.5, 1, "not a language"), "1.5");
  expect("fmtNum caches one formatter per language and digits", fmtNum(1.5, 1, "de"), fmtNum(1.5, 1, "de"));
  expect("fmtUnit days in English", fmtUnit(30, "day", "en"), "30 days");
  expect("fmtUnit days in German", fmtUnit(30, "day", "de"), "30 Tg.");
  expect("fmtUnit hours in Japanese", fmtUnit(2, "hour", "ja"), "2 時間");
  expect("fmtUnit seconds with a decimal comma", fmtUnit(1.5, "second", "de", 1), "1,5 Sek.");
  expect("fmtUnit unknown language falls back to English", fmtUnit(7, "day", "not a language"), "7 days");

  // Every call site passes its window's language, so no size reads in English by accident.
  for (const f of fs.readdirSync(uiDir).filter((n) => n.endsWith(".js"))) {
    fs.readFileSync(path.join(uiDir, f), "utf8").split(/\r?\n/).forEach((line, i) => {
      if (/\bfmtBytes\(/.test(line) && !/export function fmtBytes\(/.test(line) && !/LANG|lang/.test(line)) {
        problems.push(`ui/${f}:${i + 1} calls fmtBytes without the window language`);
      }
    });
  }

  if (problems.length) {
    console.log(`${problems.length} problem(s):`);
    for (const p of problems) console.log(" - " + p);
    process.exit(1);
  }
  console.log("fmt_num_check: ok");
}
main();
