// 附件输入的共享逻辑（面板 ⌘⇧A 与侧栏 📎 同一套语义，ADR-0010）。

import { invoke } from "@tauri-apps/api/core";

/** 附件探测结果（镜像 Rust `attachment::AttachmentInfo`，camelCase）。 */
export interface AttachmentInfo {
  name: string;
  path: string;
  kind: "text" | "image";
  bytes: number;
}

/** 校验路径（只探测类型与大小，不读入内容）；失败返回可直接展示的错误文案。 */
export function validatePath(path: string): Promise<AttachmentInfo> {
  return invoke<AttachmentInfo>("resolve_attachment", { path });
}

/** 路径 → mention 文本。引号内不支持转义，直接剔除引号字符。 */
export function mentionFor(path: string): string {
  return `@"${path.replaceAll('"', "")}"`;
}

/** 把附件 mention 追加到既有输入（与问题以空格分隔，结尾留空格便于继续输入）。 */
export function appendMention(text: string, path: string): string {
  const mention = mentionFor(path);
  const trimmed = text.trimEnd();
  return trimmed.length > 0 ? `${trimmed} ${mention} ` : `${mention} `;
}

export function humanBytes(bytes: number): string {
  if (bytes >= 1024 * 1024) {
    return `${(bytes / (1024 * 1024)).toFixed(1)} MiB`;
  }
  return `${Math.max(1, Math.ceil(bytes / 1024))} KiB`;
}
