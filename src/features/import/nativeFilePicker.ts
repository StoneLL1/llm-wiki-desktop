import type { OpenDialogOptions } from "@tauri-apps/plugin-dialog";

type DialogSelection = string | string[] | null;
type OpenDialog = (options: OpenDialogOptions) => Promise<DialogSelection>;

export function normalizeSelectedPaths(selection: DialogSelection): string[] {
  if (selection === null) return [];
  return Array.isArray(selection) ? selection : [selection];
}

export async function selectImportFiles(openDialog?: OpenDialog): Promise<string[]> {
  const open = openDialog ?? (await import("@tauri-apps/plugin-dialog")).open;
  return normalizeSelectedPaths(await open({ directory: false, multiple: true }));
}

export interface PickDirectoryOptions {
  title?: string;
}

export async function pickDirectory(
  options: PickDirectoryOptions = {},
  openDialog?: OpenDialog,
): Promise<string | null> {
  const open = openDialog ?? (await import("@tauri-apps/plugin-dialog")).open;
  const selection = await open({
    directory: true,
    multiple: false,
    ...(options.title ? { title: options.title } : {}),
  });
  const paths = normalizeSelectedPaths(selection);
  return paths[0] ?? null;
}
