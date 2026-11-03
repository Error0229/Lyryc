import "./design/index.css";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { MotionConfig } from "motion/react";
import { Overlay } from "./overlay/Overlay";
import { inTauri } from "./services/ipc/backend";
import { LlmBridge } from "./services/ipc/llm";
import { importLegacyKeys } from "./state/migrate";
import { TooltipProvider } from "./ui/Tooltip";

if (inTauri) {
  // Real bridge only: the dev fake would "accept" the keys and then forget them.
  void importLegacyKeys(localStorage, LlmBridge.importLegacyKeys).then((r) => {
    if (r === "failed") console.warn("[lyryc] legacy API keys not imported yet; kept in localStorage, will retry next launch");
  });
} else {
  const { applyOverlayScenario } = await import("./dev/scenario");
  applyOverlayScenario();
}

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <MotionConfig reducedMotion="user">
      <TooltipProvider>
        <Overlay />
      </TooltipProvider>
    </MotionConfig>
  </StrictMode>,
);
