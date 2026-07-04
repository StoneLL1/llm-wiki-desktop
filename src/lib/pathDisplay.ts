export function compactPath(path: string, maxSegments = 3): string {
  const normalized = path.replaceAll("\\", "/").replace(/\/+/g, "/");
  if (!normalized) return "";

  const unc = path.replaceAll("\\", "/").startsWith("//");
  const parts = normalized.split("/").filter(Boolean);
  const drive = /^[A-Za-z]:$/.test(parts[0] ?? "") ? parts[0] : null;
  if (unc && parts.length <= maxSegments + 1) return `//${parts.join("/")}`;
  if (drive && parts.length <= maxSegments + 1) return normalized;
  if (!unc && !drive && parts.length <= maxSegments) return normalized;

  if (unc) {
    const [server, share, ...rest] = parts;
    const tail = rest.slice(-Math.max(1, maxSegments - 1));
    return `//${server}/${share}/.../${tail.join("/")}`;
  }
  if (drive) {
    const tail = parts.slice(1).slice(-Math.max(1, maxSegments - 1));
    return `${drive}/.../${tail.join("/")}`;
  }
  const tail = parts.slice(-Math.max(1, maxSegments - 1));
  return normalized.startsWith("/") ? `/.../${tail.join("/")}` : `.../${tail.join("/")}`;
}
