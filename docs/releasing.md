# Releasing

```bash
npm version patch        # or minor / major: bumps package.json (the only version source), commits, tags vX.Y.Z
git push --follow-tags
```

The tag runs `.github/workflows/release.yml`, which builds the NSIS installer, signs it for the updater, and opens a **draft** release with `latest.json` and `lyryc-extension.zip`.

Installed apps only see published releases, so **publishing the draft is what ships the update**. Write the notes before you publish. The app's update prompt shows the `notes` field of `latest.json`, which is built from the draft's notes when it's created. If you edit the notes afterwards, change `latest.json` to match and upload it again (`gh release upload vX.Y.Z latest.json --clobber`). Only the installer is signed, so editing the manifest is safe.

Updates are signed with the key in the `TAURI_SIGNING_PRIVATE_KEY` repo secret, and its public half is in `src-tauri/tauri.conf.json`. Keep a backup of the private key. If it's lost, existing installs can't verify new updates and every user has to reinstall by hand.
