const INVALID_FOLDER_CHARS = /[<>:"/\\|?*]/g;
const WINDOWS_RESERVED_NAMES = new Set([
  "CON", "PRN", "AUX", "NUL",
  "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8", "COM9",
  "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
]);

export type ProjectNameValidationError = "required" | "invalid" | "reserved" | null;

/** Mirrors the backend's cross-platform folder-name policy before creation. */
export function validateProjectName(value: string): ProjectNameValidationError {
  const name = value.trim();
  if (!name) return "required";
  if (Array.from(name).some((char) => (char.codePointAt(0) ?? 0) < 32) || INVALID_FOLDER_CHARS.test(name)) {
    INVALID_FOLDER_CHARS.lastIndex = 0;
    return "invalid";
  }
  INVALID_FOLDER_CHARS.lastIndex = 0;
  if (name.endsWith(".") || name.endsWith(" ")) return "invalid";
  if (WINDOWS_RESERVED_NAMES.has(name.split(".", 1)[0].toUpperCase())) return "reserved";
  return null;
}

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
