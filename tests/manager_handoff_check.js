// The media manager's handoff to the main window (ui/capture.js, ui/recordingTools.js).
// Run:  node tests/manager_handoff_check.js      Exit: 0 = clean, 1 = problems.
//
// "manager-open-file" and "manager-export" are bare signals: any webview may emit an event, so
// the main window must pull the path, folder and names from the backend (which stored the ones
// it resolved) and never read them from the event payload.
const fs = require("fs");
const path = require("path");

const UI = path.join(__dirname, "..", "ui");
const problems = [];

// Text of the listen("<event>", ...) call: from its start to the next two-space-indented listen/bind line.
function listenBlock(file, event) {
  const src = fs.readFileSync(path.join(UI, file), "utf8");
  const at = src.indexOf(`listen("${event}"`);
  if (at < 0) { problems.push(`${file}: no listener for ${event}`); return ""; }
  const rest = src.slice(at + 1);
  const end = rest.search(/\n  (?:\/\/|listen\(|bind\w+\()/);
  return src.slice(at, end < 0 ? src.length : at + 1 + end);
}

for (const [file, event, command] of [
  ["capture.js", "manager-open-file", "take_manager_open_file"],
  ["recordingTools.js", "manager-export", "take_manager_export"],
]) {
  const block = listenBlock(file, event);
  if (!block) continue;
  if (/payload/.test(block)) problems.push(`${file}: ${event} handler reads the event payload`);
  if (!block.includes(`invoke("${command}")`)) problems.push(`${file}: ${event} handler does not pull ${command}`);
}

if (problems.length) {
  console.error(problems.join("\n"));
  process.exit(1);
}
console.log("manager handoff check: ok");
