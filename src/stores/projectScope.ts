let projectScopeEpoch = 0;

export function captureProjectScope(): number {
  return projectScopeEpoch;
}

export function invalidateProjectScope(): void {
  projectScopeEpoch += 1;
}

export function isProjectScopeCurrent(epoch: number): boolean {
  return epoch === projectScopeEpoch;
}
