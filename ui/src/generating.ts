// Inline "generating" indicator (three bouncing dots, follows currentColor).
// Used by the panel's AI answers and the Side View's streaming bubble alike, replacing the old practice of writing status into the body text.

export function generatingEl(label = "Generating"): HTMLElement {
  const wrap = document.createElement("span");
  wrap.className = "moe-generating";
  const dots = document.createElement("span");
  dots.className = "moe-dots";
  for (let i = 0; i < 3; i += 1) {
    dots.append(document.createElement("span"));
  }
  const text = document.createElement("span");
  text.textContent = label;
  wrap.append(dots, text);
  return wrap;
}
