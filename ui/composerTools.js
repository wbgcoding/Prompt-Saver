// Composer tool buttons: they stack into rows next to a tall text box instead of
// staying in one wide row that takes width away from the text.

const BTN = 36; // button edge, px (matches .composer-img-btn)
const ROW_GAP = 6; // vertical gap between stacked rows, px

// Pure: how many columns the tool grid needs for an input of `inputHeight` px with
// `visible` buttons. Rows follow the input height, columns follow from the rows, and
// the rows are then recomputed from the columns so the last row is never empty.
export function composerToolCols(inputHeight, visible) {
  if (!(visible > 0)) return 1;
  const fit = Math.floor(((Number(inputHeight) || 0) + ROW_GAP) / (BTN + ROW_GAP));
  const rows = Math.min(Math.max(fit, 1), visible);
  return Math.ceil(visible / rows);
}

// Pure: the column counts the grid can take for `visible` buttons, fewest first.
export function composerColChoices(visible) {
  if (!(visible > 0)) return [1];
  const out = new Set();
  for (let rows = visible; rows >= 1; rows--) out.add(Math.ceil(visible / rows));
  return [...out].sort((a, b) => a - b);
}

// Pure: the column count to use when the text box height depends on it. Stacking the
// buttons widens the text box, a wider box needs fewer lines, and a shorter box asks for
// fewer rows again; deciding on the height measured at the current layout therefore
// flips back and forth with every typed character near a line break. Instead each
// choice is judged on the height the text would have WITH that choice
// (`heightAt(cols)`), and the most stacked choice whose own height still holds its rows
// wins. More text only ever means more stacking, so typing never makes the grid jump
// back and forth.
export function pickComposerCols(visible, heightAt) {
  const choices = composerColChoices(visible);
  for (const cols of choices) {
    if (composerToolCols(heightAt(cols), visible) <= cols) return cols;
  }
  return choices[choices.length - 1];
}

let lastCols = 0;

function visibleTools(tools) {
  let visible = 0;
  for (const b of tools.children) {
    if (!b.hidden && getComputedStyle(b).display !== "none") visible++;
  }
  return visible;
}

// Picks the column count for the current text and sizes the input for it. `size(input)`
// fits the input height to its text and returns nothing; every candidate is laid out and
// measured inside this one task, so nothing in between is ever painted. Returns false
// while the composer is hidden (nothing to measure).
export function fitComposerTools(size) {
  const tools = document.querySelector(".composer-tools");
  const input = document.getElementById("input");
  if (!tools || !input || !input.offsetHeight) return false;
  const visible = visibleTools(tools);
  const setCols = (cols) => tools.style.setProperty("--tool-cols", String(cols));
  const resize = () => { if (size) size(input); };
  const cols = pickComposerCols(visible, (c) => {
    setCols(c);
    resize();
    return input.offsetHeight;
  });
  setCols(cols);
  resize();
  lastCols = cols;
  return true;
}

// The column count of the last fit, for tests.
export const composerColsNow = () => lastCols;
