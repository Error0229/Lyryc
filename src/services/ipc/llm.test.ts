import { describe, expect, it } from "vitest";
import { LLM_NOT_CONFIGURED } from "./llm";
import { friendlyError, isMissingCommand, isNotConfigured } from "./llmClient";
import { createFakeLlm } from "./llmFake";

describe("LLM error helpers", () => {
  it("recognises the not-configured prefix and strips it for display", () => {
    const err = `${LLM_NOT_CONFIGURED} no provider routed for translation`;
    expect(isNotConfigured(err)).toBe(true);
    expect(isNotConfigured(new Error(err))).toBe(true);
    expect(isNotConfigured("401 Unauthorized")).toBe(false);
    expect(friendlyError(err)).toBe("no provider routed for translation");
    expect(friendlyError("boom")).toBe("boom");
  });

  it("detects Tauri's unknown-command error", () => {
    expect(isMissingCommand("command llm_get_settings not found")).toBe(true);
    expect(isMissingCommand("network error")).toBe(false);
  });
});

describe("fake LlmBridge", () => {
  it("never returns a key, only a hint", async () => {
    const llm = createFakeLlm();
    const p = await llm.upsertProvider({ presetId: "openai", apiKey: "sk-test-abcdef123456" });
    expect(p).toMatchObject({ hasKey: true, keyHint: "sk-…3456", label: "OpenAI" });
    expect(JSON.stringify(await llm.getSettings())).not.toContain("abcdef");
  });

  it("keeps, replaces and clears keys per the contract", async () => {
    const llm = createFakeLlm();
    const p = await llm.upsertProvider({ presetId: "openai", apiKey: "sk-first-0000000001" });
    expect((await llm.upsertProvider({ id: p.id, presetId: "openai", label: "Work" })).hasKey).toBe(true);
    expect((await llm.upsertProvider({ id: p.id, presetId: "openai", apiKey: "" })).hasKey).toBe(false);
  });

  it("requires a base URL for custom presets", async () => {
    const llm = createFakeLlm();
    await expect(llm.upsertProvider({ presetId: "custom" })).rejects.toMatch(/base URL/);
    await expect(llm.upsertProvider({ presetId: "custom", baseUrl: "http://localhost:1234/v1" })).resolves.toMatchObject({ baseUrl: "http://localhost:1234/v1" });
  });

  it("drops routes that point at a removed provider", async () => {
    const llm = createFakeLlm();
    const p = await llm.upsertProvider({ presetId: "ollama" });
    await llm.setRoute("translation", { providerId: p.id, model: "llama3.2" });
    await llm.removeProvider(p.id);
    expect((await llm.getSettings()).routes).toEqual({});
  });

  it("imports legacy keys as providers", async () => {
    const llm = createFakeLlm();
    const s = await llm.importLegacyKeys({ geminiApiKey: "AIza-legacy-key-9999" });
    expect(s.providers).toHaveLength(1);
    expect(s.providers[0]).toMatchObject({ presetId: "gemini", hasKey: true });
  });
});
