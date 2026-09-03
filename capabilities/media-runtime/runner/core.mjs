import path from "node:path";
import process from "node:process";

export function restrictedEnvironment(packRoot, platform = process.platform, source = process.env) {
  const result = { NO_COLOR: "1" };
  for (const name of ["SystemRoot", "WINDIR", "TEMP", "TMP", "TMPDIR"]) {
    if (typeof source[name] === "string") result[name] = source[name];
  }
  const ffmpegLib = path.join(packRoot, "runtime", "ffmpeg", "lib");
  if (platform === "linux") result.LD_LIBRARY_PATH = ffmpegLib;
  if (platform === "darwin") result.DYLD_LIBRARY_PATH = ffmpegLib;
  if (platform === "win32") {
    const ffmpegBin = path.join(packRoot, "runtime", "ffmpeg", "bin");
    result.PATH = `${ffmpegBin}${path.delimiter}${source.PATH ?? ""}`;
  }
  return result;
}
