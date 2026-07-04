const INVALID_FOLDER_CHARS = /[<>:"/\\|?*\u0000-\u001f]/g;

export function sanitizeProjectFolderName(value: string): string {
  return value
    .trim()
    .replace(INVALID_FOLDER_CHARS, "")
    .replace(/[. ]+$/g, "")
    .trim();
}

export function buildProjectRootPath(parentPath: string, projectName: string): string {
  const parent = parentPath.trim();
  const folder = sanitizeProjectFolderName(projectName);
  if (!parent || !folder) return "";
  const separator = parent.includes("\\") && !parent.includes("/") ? "\\" : "/";
  return `${parent.replace(/[\\/]+$/g, "")}${separator}${folder}`;
}
