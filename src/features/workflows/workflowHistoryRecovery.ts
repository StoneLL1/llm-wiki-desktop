export const HISTORY_PAGE_REFRESH_CODES = [
  "WORKFLOW_CURSOR_INVALID",
  "WORKFLOW_CURSOR_SCOPE_MISMATCH",
  "WORKFLOW_HISTORY_IDENTITY_MISMATCH",
  "WORKFLOW_HISTORY_PAGE_TOO_LARGE",
] as const;

export function historyPageErrorRequiresRefresh(detailsOrCode: string | null | undefined): boolean {
  return Boolean(detailsOrCode && HISTORY_PAGE_REFRESH_CODES.some((code) => detailsOrCode.includes(code)));
}
