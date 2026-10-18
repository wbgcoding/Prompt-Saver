// The main window's JavaScript as one text, for the tests that scan its source.
// The list lives in tests/ui_sources.json (key main_window_modules, in concatenation
// order); a module created for the main window is appended there. Tests read the
// code through here, never by opening ui/main.js directly.
const fs = require("fs");
const path = require("path");

const ui = path.join(__dirname, "..", "ui");
const sources = JSON.parse(fs.readFileSync(path.join(__dirname, "ui_sources.json"), "utf8"));
const mainWindowModules = sources.main_window_modules;
// The video editor window: video.js plus the modules it is split into.
const videoWindowModules = sources.video_window_modules;

const mainWindowFiles = () => mainWindowModules.map((name) => path.join(ui, name));
const mainWindowSource = () => mainWindowFiles().map((f) => fs.readFileSync(f, "utf8")).join("\n");

const videoWindowSource = () => videoWindowModules.map((name) => fs.readFileSync(path.join(ui, name), "utf8")).join("\n");

module.exports = { mainWindowModules, mainWindowFiles, mainWindowSource, videoWindowModules, videoWindowSource };
