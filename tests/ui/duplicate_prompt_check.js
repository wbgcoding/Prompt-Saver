// Editor (ui/editor.js duplicatePrompt, real extracted source in a vm): a duplicate opens the
// create dialog with a create heading, and an image or file entry opens in its own create mode
// so the copy text follows the new name instead of keeping the original's.
// Run:  node tests/ui/duplicate_prompt_check.js      Exit: 0 = clean, 1 = problems.
const path = require("path");
const vm = require("vm");

const src = require(path.join(__dirname, "..", "ui_sources.js")).mainWindowSource().replace(/\r\n/g, "\n");
function extractBlock(marker) {
  const start = src.indexOf(marker);
  if (start === -1) throw new Error(`could not find (source text changed?): ${marker}`);
  const braceStart = src.indexOf("{", src.indexOf(")", start));
  let depth = 0;
  for (let i = braceStart; i < src.length; i++) {
    if (src[i] === "{") depth++;
    else if (src[i] === "}" && --depth === 0) return src.slice(start, i + 1);
  }
  throw new Error(`unbalanced braces after: ${marker}`);
}
const code = [extractBlock("function promptToModal("), extractBlock("async function duplicatePrompt(")].join("\n");

async function open(p) {
  let opened = null;
  const sandbox = {
    invoke: async () => p, openModal: (o) => { opened = o; }, t: (k) => k,
    errToast() {}, ensureSecretVaultUnlocked: async () => false,
    mediaKind: (f) => (/\.mp4$/.test(f) ? "video" : /\.png$/.test(f) ? "image" : ""),
    PDF_EXT: /\.pdf$/i,
  };
  vm.createContext(sandbox);
  vm.runInContext(code + "\nthis.duplicatePrompt = duplicatePrompt;", sandbox);
  await sandbox.duplicatePrompt("id1");
  return opened;
}

async function main() {
  const problems = [];
  const text = await open({ name: "A", text: "body" });
  if (text.mode !== "create") problems.push(`text duplicate mode ${text.mode}`);
  if (text.title !== "nameModalTitle") problems.push(`text duplicate heading ${text.title}`);
  const img = await open({ name: "A", text: "A", image: "data:x", copy_image: true });
  if (img.mode !== "image-create") problems.push(`image duplicate mode ${img.mode}`);
  if (img.title !== "imageModalTitle") problems.push(`image duplicate heading ${img.title}`);
  const vid = await open({ name: "A", text: "A", file_path: "C:/a.mp4" });
  if (vid.mode !== "file-create") problems.push(`video duplicate mode ${vid.mode}`);
  if (vid.title !== "videoModalTitle") problems.push(`video duplicate heading ${vid.title}`);
  const doc = await open({ name: "A", text: "A", file_path: "C:/a.docx" });
  if (doc.mode !== "file-create" || doc.title !== "fileModalTitle") problems.push(`file duplicate ${doc.mode} / ${doc.title}`);
  if (problems.length) {
    console.error("duplicate_prompt_check: FAILED\n  " + problems.join("\n  "));
    process.exit(1);
  }
  console.log("duplicate_prompt_check: ok");
}
main().catch((e) => { console.error(e); process.exit(1); });
