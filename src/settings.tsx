import "./design/index.css";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { MotionConfig } from "motion/react";
import { SettingsApp } from "./settings/SettingsApp";
import { inTauri } from "./services/ipc/backend";
import { TooltipProvider } from "./ui/Tooltip";

if (!inTauri) {
  const { applySettingsScenario } = await import("./dev/scenario");
  await applySettingsScenario();
}

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <MotionConfig reducedMotion="user">
      <TooltipProvider>
        <SettingsApp />
      </TooltipProvider>
    </MotionConfig>
  </StrictMode>,
);
