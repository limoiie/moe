// 「正在生成」的行内指示（三点跳动，随 currentColor）。
// 面板的 AI 回答与侧栏的流式气泡都用它，替代过去把状态写成正文文案的做法。

export function generatingEl(label = "正在生成"): HTMLElement {
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
