// Signs a release installer for the auto-updater.
//
// The updater accepts an installer only when "<installer>.sig" holds a valid Ed25519
// signature over the installer's SHA-256 digest, made with the private key whose public
// half is built into the app (UPDATE_PUBLIC_KEY in src-tauri/src/update.rs). The private
// key lives outside the repository and is never printed.
//
//   node tools/sign_update.js init            create the key pair once, print the public key
//   node tools/sign_update.js pub             print the public key
//   node tools/sign_update.js sign <file>     write <file>.sig
//   node tools/sign_update.js verify <file>   check <file>.sig against the built-in key
//
// Key location: PS_UPDATE_KEY, or %USERPROFILE%\.prompt-saver-signing\update-key.pem.
// Keep a backup of that file: without it, users of a signed build can only update by hand.

const crypto = require("crypto");
const fs = require("fs");
const os = require("os");
const path = require("path");

const KEY_PATH = process.env.PS_UPDATE_KEY || path.join(os.homedir(), ".prompt-saver-signing", "update-key.pem");
const UPDATE_RS = path.join(__dirname, "..", "src-tauri", "src", "update.rs");

function fail(msg) {
  console.error(msg);
  process.exit(1);
}

function rawPublicKey(key) {
  // The SPKI DER of an Ed25519 key ends with the 32 raw key bytes.
  const der = crypto.createPublicKey(key).export({ type: "spki", format: "der" });
  return der.subarray(der.length - 32);
}

function loadPrivateKey() {
  if (!fs.existsSync(KEY_PATH)) fail(`No signing key at ${KEY_PATH}. Run "node tools/sign_update.js init" once.`);
  return crypto.createPrivateKey(fs.readFileSync(KEY_PATH));
}

function digestOf(file) {
  return crypto.createHash("sha256").update(fs.readFileSync(file)).digest();
}

function builtInPublicKey() {
  const src = fs.readFileSync(UPDATE_RS, "utf8");
  const m = src.match(/UPDATE_PUBLIC_KEY_B64: &str = "([A-Za-z0-9+/=]+)"/);
  if (!m) fail("UPDATE_PUBLIC_KEY_B64 not found in update.rs");
  return Buffer.from(m[1], "base64");
}

const [cmd, file] = process.argv.slice(2);
if (cmd === "init") {
  if (fs.existsSync(KEY_PATH)) fail(`A key already exists at ${KEY_PATH}; refusing to replace it.`);
  const { privateKey } = crypto.generateKeyPairSync("ed25519");
  fs.mkdirSync(path.dirname(KEY_PATH), { recursive: true });
  fs.writeFileSync(KEY_PATH, privateKey.export({ type: "pkcs8", format: "pem" }), { mode: 0o600 });
  console.log(rawPublicKey(privateKey).toString("base64"));
} else if (cmd === "pub") {
  console.log(rawPublicKey(loadPrivateKey()).toString("base64"));
} else if (cmd === "sign") {
  if (!file || !fs.existsSync(file)) fail("usage: sign <installer>");
  const key = loadPrivateKey();
  if (!rawPublicKey(key).equals(builtInPublicKey())) fail("The signing key does not match the public key built into update.rs.");
  const sig = crypto.sign(null, digestOf(file), key);
  fs.writeFileSync(`${file}.sig`, sig.toString("base64"));
  console.log(`signed ${path.basename(file)}`);
} else if (cmd === "verify") {
  if (!file || !fs.existsSync(`${file}.sig`)) fail("usage: verify <installer> (needs <installer>.sig)");
  const pub = crypto.createPublicKey({
    key: Buffer.concat([Buffer.from("302a300506032b6570032100", "hex"), builtInPublicKey()]),
    format: "der",
    type: "spki",
  });
  const sig = Buffer.from(fs.readFileSync(`${file}.sig`, "utf8").trim(), "base64");
  const ok = crypto.verify(null, digestOf(file), pub, sig);
  console.log(ok ? "signature ok" : "signature INVALID");
  process.exit(ok ? 0 : 1);
} else {
  fail("usage: node tools/sign_update.js init | pub | sign <file> | verify <file>");
}
