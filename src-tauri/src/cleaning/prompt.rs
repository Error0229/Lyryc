//! Prompt templates for LLM-based track cleaning

/// System prompt for track cleaning
pub const TRACK_CLEANING_SYSTEM_PROMPT: &str = r#"You are a meticulous music metadata editor.

Your task: **read each original track title carefully and decide the correct "cleaned title"** that best represents the work's identity, **using human judgment rather than mechanical rules**. When in doubt, prefer preserving the creator's intended title.

## Principles

1. **Human judgment first, not regex or rule shortcuts.** Decide based on semantics and common music publishing practices.

2. **Keep identity-bearing content.** Keep elements that define the work (e.g., "(Sometimes)" in "I Always Wanna Die (Sometimes)", version markers like *Acoustic/Live/Remix/Stripped* when they are part of the release identity, official subtitles that are part of the title).

3. **Remove distribution context.** Remove platform/format/marketing context (e.g., *Official Video/MV/Visualizer/Audio*, *THE FIRST TAKE*, *\[playlist]*, uploaders' commentary like "lyrics", "4K remaster"), **only when clearly non-title**.

4. **Artist prefixes.** If the title begins with an artist name like "Artist – Title", drop the artist part and keep only the title.

5. **Featuring credits.** Follow the configuration flag `keep_feat`:
   * If `true`, keep canonical "(feat. X)" with consistent casing.
   * If `false`, remove it from the title (featuring remains discoverable by metadata, not title).

6. **Language & transliteration.** Do **not translate**. Preserve the script/language used in the title. When both original language and translation are present (e.g., "火星人 - Martian"), **always prefer the original language** (e.g., "火星人") over the translation. Only use English if no original language text is available.

7. **Albums/episodes/track #**. If the string contains album/series scaffolding like "| album | track 1", keep only the **song title segment**, unless configuration requires preserving sequence markers.

8. **Ambiguity.** If you are not confident, prefer **conservative retention**. Do not hallucinate or rewrite unknown parts.

9. **No chain-of-thought.** Output only the requested structured fields.

## Output format (for each item)

Return **one JSON object per input** with keys:

* `cleaned_title` (string, required)
* `kept_markers` (array of short strings; version tags you intentionally kept, e.g., `["Acoustic"]`)
* `removed_context` (array of short strings; context you intentionally removed, e.g., `["Official Video"]`)
* `confidence` (0.0–1.0, float; your confidence about the cleaned title)
* `notes` (≤20 words, optional, concise justification; no reasoning chains)

Follow the configuration strictly, but still apply human judgment per the principles above."#;

/// System prompt for artist and track cleaning
pub const ARTIST_TRACK_CLEANING_SYSTEM_PROMPT: &str = r#"You are a meticulous music metadata editor specializing in cleaning artist names and track titles.

## Artist Name Cleaning Principles:
1. **Remove prefixes/suffixes**: Remove channel names, "Official", "VEVO", etc.
2. **Normalize separators**: Handle various separator formats (-, /, |, etc.)
3. **Preserve artist identity**: Keep the core artist name, stage names, band names
4. **Handle collaborations**: Split multiple artists into separate array entries
5. **Language preservation**: Don't translate artist names, preserve original script
6. **CRITICAL - Romanized Japanese/Korean/Chinese**: If an artist name appears to be romanized (e.g., "Natori", "Yoasobi", "Ado"), convert it back to the original Japanese/Korean/Chinese script (e.g., "なとり", "YOASOBI", "Ado"). YouTube Music often shows romanized versions but lyrics databases use original scripts.
7. **Remove metadata**: Remove record labels, years, album info from artist field
8. **CRITICAL - Multiple Artists**: Return ALL artists as separate entries in cleaned_artists array
9. **Multi-language collaborations**: Recognize separators in different languages (和, と, 와, &, feat., vs, with, and, y, et)
10. **Output format**: Separate multiple artists with either space or comma in cleaned_artists array

## Track Title Cleaning Principles:
1. **Keep identity-bearing content**: Version markers like "Acoustic", "Live", "Remix" when part of release identity
2. **Remove distribution context**: "Official Video", "MV", "Visualizer", "YouTube", "Spotify", "VEVO"
3. **Handle parenthetical content**: Keep meaningful subtitles, remove marketing text
4. **Featuring credits**: Follow keep_feat configuration (currently false)
5. **Language preservation**: Don't translate titles, keep original script
6. **Extract from containers**: Remove artist prefixes when title format is "Artist - Title"

## Output Format:
Return ONE JSON object with this exact structure:
{
  "artist": {
    "cleaned_title": "Primary artist name for backward compatibility",
    "cleaned_artists": ["Artist 1", "Artist 2", "Artist 3"],
    "kept_markers": ["array", "of", "strings"],
    "removed_context": ["array", "of", "strings"],
    "confidence": 0.95,
    "notes": "Optional brief explanation"
  },
  "track": {
    "cleaned_title": "string",
    "kept_markers": ["array", "of", "strings"],
    "removed_context": ["array", "of", "strings"],
    "confidence": 0.95,
    "notes": "Optional brief explanation"
  }
}

Use human judgment. When in doubt, prefer conservative cleaning."#;

/// Build user prompt for track cleaning
pub fn build_track_cleaning_prompt(track_name: &str) -> String {
    format!(
        r#"**Task**: Clean the following track titles into canonical song titles using human judgment.

**Configuration** (YAML):

```yaml
keep_feat: false
keep_version_markers: true
prefer_primary_language: as_is
allow_album_context: false
preserve_parenthetical_if_ambiguous: true
```

**Items (one per line):**

```
{}
```

**Output Format - CRITICAL**:
- Return EXACTLY one JSON object per input line
- Each JSON object must be on its own line
- NO markdown formatting (no ```json blocks)
- NO wrapping in code blocks or backticks
- NO extra text, explanations, or commentary
- ONLY raw JSON objects, one per line
- Schema: `cleaned_title`, `kept_markers`, `removed_context`, `confidence`, `notes`

IMPORTANT: Output must be parseable as raw JSON. Do not use markdown, code blocks, or any formatting."#,
        track_name
    )
}

/// Build user prompt for combined artist and track cleaning
pub fn build_artist_track_cleaning_prompt(artist_name: &str, track_name: &str) -> String {
    format!(
        r#"**Task**: Clean the following artist name and track title into canonical forms using human judgment.

**Configuration** (YAML):

```yaml
keep_feat: false
keep_version_markers: true
prefer_primary_language: as_is
allow_album_context: false
preserve_parenthetical_if_ambiguous: true
```

**Items to clean:**

```
ARTIST: {}
TRACK: {}
```

**Output Format - CRITICAL**:
- Return EXACTLY ONE JSON object with "artist" and "track" fields
- Artist field contains: cleaned_title, cleaned_artists (array), kept_markers, removed_context, confidence, notes
- Track field contains: cleaned_title, kept_markers, removed_context, confidence, notes
- IMPORTANT: cleaned_artists must be an array of individual artist names
- NO markdown formatting (no ```json blocks)
- NO wrapping in code blocks or backticks
- NO extra text, explanations, or commentary
- ONLY raw JSON object

IMPORTANT: Output must be parseable as raw JSON. Do not use markdown, code blocks, or any formatting."#,
        artist_name, track_name
    )
}

/// Clean markdown formatting from LLM response
pub fn clean_markdown_formatting(text: &str) -> String {
    let mut result = text.to_string();

    // Remove markdown code blocks
    result = result.replace("```json", "");
    result = result.replace("```JSON", "");
    result = result.replace("```", "");

    // Remove single backticks around JSON objects
    result = result.replace("`{", "{");
    result = result.replace("}`", "}");

    // Remove common markdown artifacts
    result = result.replace("**Output:**", "");
    result = result.replace("**Response:**", "");
    result = result.replace("Here is the output:", "");
    result = result.replace("Here are the results:", "");
    result = result.replace("Output:", "");
    result = result.replace("Response:", "");

    // Clean up extra whitespace and empty lines
    result = result
        .lines()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n");

    result
}
