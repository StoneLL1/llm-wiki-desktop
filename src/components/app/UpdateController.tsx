import { lazy, Suspense } from "react";

import { useUpdateController } from "../../features/update/useUpdateController";
import { useUpdateStore } from "../../stores/updateStore";
import { ViewErrorBoundary } from "./ViewErrorBoundary";

const UpdateDialog = lazy(async () => {
  const module = await import("../../features/update/UpdateDialog");
  return { default: module.UpdateDialog };
});

export function UpdateController() {
  useUpdateController();
  const dialogOpen = useUpdateStore((state) => state.dialogOpen);
  if (!dialogOpen) return null;
  return (
    <ViewErrorBoundary>
      <Suspense fallback={null}>
        <UpdateDialog />
      </Suspense>
    </ViewErrorBoundary>
  );
}
