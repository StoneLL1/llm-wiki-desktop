/* global process */
// The caller supplies only an already policy-approved Bilibili URL.
const forbidden=["--exec","--external-downloader","--netrc","--cookies-from-browser","--plugin-dirs","--output","--postprocessor-args"];
const fixedArgs=["--dump-single-json","--skip-download","--write-subs","--write-auto-subs","--no-playlist","--no-config","--ignore-config"];
if(process.argv.slice(2).some(a=>forbidden.some(f=>a===f||a.startsWith(f+"="))))process.exit(64);
process.stdout.write(JSON.stringify({fixedArgs,releasePayloadRequired:true})+"\n");
