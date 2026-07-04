const INVALID_FOLDER_CHARS = /[<>:"/\\|?*]/g;

export function sanitizeProjectFolderName(value: string): string {
  const withoutControlChars = Array.from(value)
    .filter((char) => (char.codePointAt(0) ?? 0) >= 32)
    .join("");
  return withoutControlChars
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
