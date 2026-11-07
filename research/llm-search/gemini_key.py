"""Read the Gemini key the user saved in Lyryc (never printed, logged or written).

Lyryc keeps keys in Windows Credential Manager (target `<provider id>.com.lyryc.app`,
UTF-16 blob, as written by keyring-rs); older settings files still hold `apiKey`.
"""
import ctypes, json, os
from ctypes import wintypes
from pathlib import Path


class _Credential(ctypes.Structure):
    _fields_ = [
        ("Flags", wintypes.DWORD), ("Type", wintypes.DWORD), ("TargetName", wintypes.LPWSTR),
        ("Comment", wintypes.LPWSTR), ("LastWritten", wintypes.FILETIME),
        ("CredentialBlobSize", wintypes.DWORD), ("CredentialBlob", ctypes.POINTER(ctypes.c_ubyte)),
        ("Persist", wintypes.DWORD), ("AttributeCount", wintypes.DWORD), ("Attributes", ctypes.c_void_p),
        ("TargetAlias", wintypes.LPWSTR), ("UserName", wintypes.LPWSTR),
    ]


def _credential(target):
    advapi = ctypes.WinDLL("advapi32", use_last_error=True)
    cred = ctypes.POINTER(_Credential)()
    if not advapi.CredReadW(target, 1, 0, ctypes.byref(cred)):  # 1 = CRED_TYPE_GENERIC
        return None
    try:
        c = cred.contents
        return bytes(c.CredentialBlob[: c.CredentialBlobSize]).decode("utf-16-le")
    finally:
        advapi.CredFree(cred)


def gemini_key():
    path = Path(os.environ.get("LYRYC_LLM_SETTINGS") or Path(os.environ["APPDATA"]) / "com.lyryc.app/llm-settings.json")
    settings = json.loads(path.read_text(encoding="utf-8"))
    for p in settings["providers"]:
        if p["presetId"] != "gemini":
            continue
        key = p.get("apiKey") or (p.get("inKeyring") and _credential(f"{p['id']}.com.lyryc.app"))
        if key:
            return key
    raise SystemExit("no Gemini provider with a key in " + str(path))
