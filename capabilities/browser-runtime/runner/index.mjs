/* global process */
// Release builds replace this fail-closed shim with the pinned Playwright archive.
// Fixed policy: dedicated userDataDir, --disable-extensions, acceptDownloads:false,
// no exposed remote-debugging port, popups denied, and every request checked by Rust.
process.stderr.write("signed Playwright payload required\n");process.exit(78);
