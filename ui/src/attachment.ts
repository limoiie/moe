// Shared attachment input logic (the panel's ⌘⇧A and the Side View's 📎 use the same semantics, ADR-0010).

import { invoke } from "@tauri-apps/api/core";

/** Attachment probe result (mirrors Rust `attachment::AttachmentInfo`, camelCase). */
export interface AttachmentInfo {
  name: string;
  path: string;
  kind: "text" | "image";
  bytes: number;
}

/** Validate a path (probes type and size only, never reads content); on failure returns an error message ready to display. */
export function validatePath(path: string): Promise<AttachmentInfo> {
  return invoke<AttachmentInfo>("resolve_attachment", { path });
}

/** Path → mention text. Quotes inside the path cannot be escaped, so they are stripped. */
export function mentionFor(path: string): string {
  return `@"${path.replaceAll('"', "")}"`;
}

/** Append an attachment mention to existing input (space-separated from the question; a trailing space is left for continued typing). */
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
