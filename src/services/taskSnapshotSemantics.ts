import type { BackendTask } from "../types/task";

function structurallyEqual(left: unknown, right: unknown): boolean {
  if (Object.is(left, right)) return true;
  if (typeof left !== typeof right || left === null || right === null) return false;
  if (Array.isArray(left) || Array.isArray(right)) {
    if (!Array.isArray(left) || !Array.isArray(right) || left.length !== right.length) return false;
    return left.every((entry, index) => structurallyEqual(entry, right[index]));
  }
  if (typeof left !== "object") return false;
  const leftRecord = left as Record<string, unknown>;
  const rightRecord = right as Record<string, unknown>;
  const leftKeys = Object.keys(leftRecord);
  const rightKeys = Object.keys(rightRecord);
  if (leftKeys.length !== rightKeys.length) return false;
  return leftKeys.every((key) => (
    Object.prototype.hasOwnProperty.call(rightRecord, key)
    && structurallyEqual(leftRecord[key], rightRecord[key])
  ));
}

export function isBackendTaskSnapshot(value: unknown): value is BackendTask {
  if (typeof value !== "object" || value === null) return false;
  const candidate = value as Partial<BackendTask>;
  return typeof candidate.id === "string"
    && typeof candidate.status === "string"
    && typeof candidate.updatedAt === "string"
    && typeof candidate.title === "string";
}

export function taskSnapshotsEqual(left: BackendTask, right: BackendTask): boolean {
  return structurallyEqual(left, right);
}

export function isProgressOnlyTaskSnapshot(previous: BackendTask, incoming: BackendTask): boolean {
  if (taskSnapshotsEqual(previous, incoming)) return false;
  const {
    progress: previousProgress,
    updatedAt: previousUpdatedAt,
    ...previousStable
  } = previous;
  const {
    progress: incomingProgress,
    updatedAt: incomingUpdatedAt,
    ...incomingStable
  } = incoming;
  return structurallyEqual(previousStable, incomingStable)
    && (!structurallyEqual(previousProgress, incomingProgress) || previousUpdatedAt !== incomingUpdatedAt);
}
