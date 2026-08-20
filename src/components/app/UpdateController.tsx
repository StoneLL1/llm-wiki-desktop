import { UpdateDialog } from "../../features/update/UpdateDialog";
import { useUpdateController } from "../../features/update/useUpdateController";

export function UpdateController() {
  useUpdateController();
  return <UpdateDialog />;
}
