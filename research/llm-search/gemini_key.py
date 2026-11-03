"""Read the Gemini key the user saved in Lyryc (never printed, logged or written)."""
import json, os
from pathlib import Path


def gemini_key():
    path = Path(os.environ.get("LYRYC_LLM_SETTINGS") or Path(os.environ["APPDATA"]) / "com.lyryc.app/llm-settings.json")
    settings = json.loads(path.read_text(encoding="utf-8"))
    for p in settings["providers"]:
        if p["presetId"] == "gemini" and p.get("apiKey"):
            return p["apiKey"]
    raise SystemExit("no Gemini provider with a key in " + str(path))
