/**
 * Browser-only setup for building and screenshotting without Tauri. Never runs
 * inside the app. URL params (overlay):
 *   ?scenario=idle|playing|translation|furigana   see services/ipc/mock.ts
 *   &backdrop=dark|light                          fake desktop behind the strip
 * Settings window:
 *   ?theme=light|dark   &page=<settings page>     &providers (seed the fake LLM)
 */
import { mockScenario } from "../services/ipc/mock";
import { llm } from "../services/ipc/llmClient";
import { settings } from "../state/settings";

const params = new URLSearchParams(location.search);

export function applyOverlayScenario() {
  const backdrop = params.get("backdrop") ?? "dark";
  document.body.style.background = backdrop === "light" ? "#d8dbe0" : "#202329";
  if (!params.has("scenario")) return;
  settings.translation({ enabled: mockScenario === "translation" || params.has("translate") });
  settings.furigana({ enabled: mockScenario === "furigana" });
}

export async function applySettingsScenario() {
  const theme = params.get("theme");
  if (theme === "light" || theme === "dark") settings.setTheme(theme);
  if (!params.has("providers")) return;
  const openai = await llm.upsertProvider({ presetId: "openai", apiKey: "sk-demo-1234567890abcd" });
  const local = await llm.upsertProvider({ presetId: "ollama", label: "Ollama (desktop)" });
  await llm.setRoute("translation", { providerId: openai.id, model: "gpt-4.1-mini" });
  await llm.setRoute("cleaning", { providerId: local.id, model: "llama3.2" });
}
