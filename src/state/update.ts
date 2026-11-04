/** The self-updater's state, live in whichever window asks. The backend owns it. */
import { useEffect, useState } from "react";
import type { UpdateStatus } from "../lib/types";
import { backend } from "../services/ipc/backend";

export function useUpdateStatus(): UpdateStatus {
  const [status, setStatus] = useState<UpdateStatus>({ state: "idle" });
  useEffect(() => {
    let live = true;
    const off = backend.subscribe({ update: setStatus });
    backend.updateStatus().then((s) => live && setStatus(s), () => {});
    return () => {
      live = false;
      off();
    };
  }, []);
  return status;
}

/** Restarts the app on success; a failure shows up as an `error` status. */
export function installUpdate(): void {
  backend.updateInstall().catch(() => {});
}
