// numberField (ui/floatShared.js, real module): Shift+wheel arrives as deltaX with deltaY 0 on
// Chromium for Windows and must still step in the wheel's direction, and a second pointerdown
// on a stepper while the first one's hold timer is pending must not leave an orphaned timer
// that later starts an auto-repeat nobody stops.
// Run:  node tests/ui/number_field_input_check.js      Exit: 0 = clean, 1 = problems.
const path = require("path");
const { pathToFileURL } = require("url");

function mk(tag) {
  const h = {};
  const el = {
    tag, className: "", textContent: "", children: [], dataset: {}, disabled: false, value: "", min: "", max: "", step: "", attrs: {},
    classList: { add() {}, remove() {}, contains: () => false, toggle() {} },
    append(...c) { el.children.push(...c); }, appendChild(c) { el.children.push(c); return c; },
    setAttribute(k, v) { el.attrs[k] = v; },
    addEventListener(t, f) { (h[t] ||= []).push(f); },
    removeEventListener() {},
    dispatchEvent(ev) { for (const f of h[ev.type] || []) f(ev); return true; },
    fire(type, ev = {}) { for (const f of h[type] || []) f({ type, preventDefault() {}, button: 0, pointerId: 1, ...ev }); },
    setPointerCapture() {},
  };
  return el;
}

async function main() {
  globalThis.document = { createElement: mk, addEventListener() {}, removeEventListener() {} };
  const shared = await import(pathToFileURL(path.join(__dirname, "..", "..", "ui", "floatShared.js")).href);
  const problems = [];

  // Wheel: deltaY, or deltaX only (Shift+wheel).
  let input = mk("input");
  shared.numberField({ input, value: 10, min: 0, max: 100, step: 1, bigStep: 10 });
  input.fire("wheel", { deltaY: -100, deltaX: 0, shiftKey: false });
  if (input.value !== "11") problems.push(`wheel up with deltaY gave ${input.value}, expected 11`);
  input.fire("wheel", { deltaY: 0, deltaX: -100, shiftKey: true });
  if (input.value !== "21") problems.push(`Shift+wheel up (deltaX only) gave ${input.value}, expected 21`);
  input.fire("wheel", { deltaY: 0, deltaX: 100, shiftKey: true });
  if (input.value !== "11") problems.push(`Shift+wheel down (deltaX only) gave ${input.value}, expected 11`);
  input.fire("wheel", { deltaY: 0, deltaX: 0, shiftKey: false });
  if (input.value !== "11") problems.push(`a wheel event with no delta moved the value to ${input.value}`);

  // Two pointers down on the plus button before the hold timer fires, then both lift.
  input = mk("input");
  const wrap = shared.numberField({ input, value: 0, min: 0, max: 100, step: 1 });
  const plus = wrap.children[2];
  plus.fire("pointerdown", { pointerId: 1 });
  await new Promise((r) => setTimeout(r, 100));
  plus.fire("pointerdown", { pointerId: 2 });
  plus.fire("pointerup", { pointerId: 2 });
  plus.fire("pointerup", { pointerId: 1 });
  const afterTaps = input.value;
  await new Promise((r) => setTimeout(r, 900));
  if (input.value !== afterTaps) problems.push("an auto-repeat kept running after both pointers lifted (" + afterTaps + " -> " + input.value + ")");
  finish(problems);
}

function finish(problems) {
  if (problems.length) {
    console.error("number_field_input_check: FAILED\n  " + problems.join("\n  "));
    process.exit(1);
  }
  console.log("number_field_input_check: ok");
  process.exit(0);
}
main().catch((e) => { console.error(e); process.exit(1); });
