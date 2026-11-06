import "./design/index.css";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { inTauri } from "./services/ipc/backend";
import { forwardLogs } from "./services/ipc/logForward";
import { TrayMenu } from "./tray/TrayMenu";

if (inTauri) forwardLogs("tray");

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <TrayMenu />
  </StrictMode>,
);
