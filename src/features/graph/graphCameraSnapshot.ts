import type { GraphCameraSnapshot } from "../../hooks/useRouteScrollRestoration";

export interface GraphCameraStateLike {
  x: number;
  y: number;
  ratio: number;
  angle: number;
}

export interface GraphCameraLike {
  getState: () => GraphCameraStateLike;
  setState: (state: GraphCameraStateLike) => void;
}

export interface GraphCameraSnapshotGate {
  invalidate: () => void;
  noteUserIntent: () => void;
  clearUserIntent: () => void;
  cameraUpdated: () => boolean;
  canCapture: () => boolean;
}

/**
 * Keeps explicit fit/reset invalidation durable across the programmatic camera
 * updates emitted by Sigma animations. A later user-owned camera gesture is
 * the only event that re-enables persistence.
 */
export function createGraphCameraSnapshotGate(): GraphCameraSnapshotGate {
  let invalidated = false;
  let userIntent = false;

  return {
    invalidate: () => {
      invalidated = true;
      userIntent = false;
    },
    noteUserIntent: () => {
      userIntent = true;
    },
    clearUserIntent: () => {
      userIntent = false;
    },
    cameraUpdated: () => {
      if (!invalidated) return true;
      if (!userIntent) return false;
      invalidated = false;
      userIntent = false;
      return true;
    },
    canCapture: () => !invalidated,
  };
}

function isValidGraphCameraSnapshot(snapshot: GraphCameraSnapshot): boolean {
  return Boolean(
    snapshot.contentHash
      && Number.isFinite(snapshot.x)
      && Number.isFinite(snapshot.y)
      && Number.isFinite(snapshot.ratio)
      && snapshot.ratio > 0
      && Number.isFinite(snapshot.angle),
  );
}

export function captureGraphCameraSnapshot(
  contentHash: string,
  camera: Pick<GraphCameraLike, "getState">,
): GraphCameraSnapshot | null {
  const snapshot = { contentHash, ...camera.getState() };
  return isValidGraphCameraSnapshot(snapshot) ? snapshot : null;
}

export function restoreGraphCameraSnapshot(
  contentHash: string,
  camera: Pick<GraphCameraLike, "setState">,
  snapshot: GraphCameraSnapshot | null,
): boolean {
  if (
    !snapshot
    || snapshot.contentHash !== contentHash
    || !isValidGraphCameraSnapshot(snapshot)
  ) return false;

  camera.setState({
    x: snapshot.x,
    y: snapshot.y,
    ratio: snapshot.ratio,
    angle: snapshot.angle,
  });
  return true;
}
