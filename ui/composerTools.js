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

let lastCols = 0;

// Reads the input's rendered height and the visible buttons, then sets --tool-cols on
// the tools block, touching the DOM only when the column count changed.
export function fitComposerTools() {
  const tools = document.querySelector(".composer-tools");
  const input = document.getElementById("input");
  if (!tools || !input || !input.offsetHeight) return; // composer hidden: nothing to measure
  let visible = 0;
  for (const b of tools.children) {
    if (!b.hidden && getComputedStyle(b).display !== "none") visible++;
  }
  const cols = composerToolCols(input.offsetHeight, visible);
  if (cols === lastCols && tools.style.getPropertyValue("--tool-cols")) return;
  lastCols = cols;
  tools.style.setProperty("--tool-cols", String(cols));
}
