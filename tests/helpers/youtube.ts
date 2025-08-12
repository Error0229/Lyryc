import path from "node:path";
import fs from "node:fs";
import { spawn } from "node:child_process";
import { fileURLToPath } from "node:url";
import { Innertube, UniversalCache } from "youtubei.js";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const TEMP_DIR = path.resolve(__dirname, "../../temp");

export interface YouTubeInfo {
  title: string;
  author?: string;
  durationSec: number;
}

export async function getYouTubeInfo(url: string): Promise<YouTubeInfo> {
  try {
    const youtube = await Innertube.create();
    const video_id = extractVideoId(url);
    const info = await youtube.getInfo(video_id);
    const duration = info.basic_info.duration?.seconds_total || 0;
    return {
      title: info.basic_info.title || 'Unknown',
      author: info.basic_info.channel?.name,
      durationSec: duration
    };
  } catch (e) {
    console.warn('Failed to get YouTube info:', e);
    throw e;
  }
}

export async function downloadAudioIfMissing(url: string, name = "test-audio"): Promise<string | null> {
  try {
    if (!fs.existsSync(TEMP_DIR)) fs.mkdirSync(TEMP_DIR, { recursive: true });

    // Check for existing files with various extensions
    const extensions = ['.webm', '.opus', '.m4a', '.mp3'];
    for (const ext of extensions) {
      const existingPath = path.join(TEMP_DIR, `${name}${ext}`);
      if (fs.existsSync(existingPath)) {
        const stat = fs.statSync(existingPath);
        if (stat.size > 1024 * 1024) { // Require >1MB for real music
          // Verify it's actually audio content, not just a large file
          const isValid = await validateAudioFile(existingPath);
          if (isValid) return existingPath;
        }
        try { fs.unlinkSync(existingPath); } catch { }
      }
    }

    const outPath = path.join(TEMP_DIR, `${name}.webm`);

    // Try YouTube.js first (primary method)
    try {
      const ok = await downloadWithYouTubeJS(url, outPath);
      if (ok) {
        const isValid = await validateAudioFile(outPath);
        if (isValid) return outPath;
        try { fs.unlinkSync(outPath); } catch { }
      }
    } catch (e) {
      console.warn('YouTube.js download failed:', e);
    }

    // Fallback to yt-dlp if available
    try {
      const hasYtDlp = await isCommandAvailable("yt-dlp");
      if (hasYtDlp) {
        const ok = await downloadWithYtDlp(url, outPath);
        if (ok) {
          const isValid = await validateAudioFile(outPath);
          if (isValid) return outPath;
          try { fs.unlinkSync(outPath); } catch { }
        }
      }
    } catch (e) {
      console.warn('yt-dlp download failed:', e);
    }

    // Don't generate fake audio - fail if real download fails
    console.warn("All download methods failed for:", url);
    return null;
  } catch {
    return null;
  }
}

async function isCommandAvailable(cmd: string): Promise<boolean> {
  return new Promise((resolve) => {
    const p = spawn(cmd, ["--version"], { stdio: "ignore" });
    p.on("error", () => resolve(false));
    p.on("exit", (code) => resolve(code === 0));
  });
}

async function downloadWithYtDlp(url: string, outPath: string): Promise<boolean> {
  return new Promise((resolve) => {
    // yt-dlp with -x (extract audio) will change the extension, so we need to handle that
    const basePattern = outPath.replace(/\.[^.]+$/, ''); // Remove extension

    const p = spawn("yt-dlp", [
      "-f", "ba", // best audio
      "-o", basePattern + ".%(ext)s", // Let yt-dlp choose extension
      "--no-playlist",
      "--no-warnings",
      "-x", // extract audio
      "--audio-format", "best", // keep best audio format
      url,
    ], { stdio: "inherit" });

    p.on("error", () => resolve(false));
    p.on("exit", (code) => {
      if (code !== 0) {
        resolve(false);
        return;
      }

      // Check for output files with common audio extensions
      const extensions = ['.opus', '.m4a', '.webm', '.mp3', '.ogg'];
      for (const ext of extensions) {
        const possiblePath = basePattern + ext;
        if (fs.existsSync(possiblePath)) {
          resolve(true);
          return;
        }
      }
      resolve(false);
    });
  });
}

function extractVideoId(url: string): string | null {
  let match = url.match(/[?&]v=([^&]+)/);
  if (match) {
    return match[1];
  }
  // Look for youtu.be/VIDEO_ID
  match = url.match(/youtu.be\/([^?]+)/);
  if (match) {
    return match[1];
  }
  return null;
}

async function downloadWithYouTubeJS(url: string, outPath: string): Promise<boolean> {
  try {
    const youtube = await Innertube.create(
      {
        cache: new UniversalCache(false),
        generate_session_locally: true,
      }
    );
    const videoId = extractVideoId(url);
    if (!videoId) {
      console.warn('Invalid YouTube URL:', url);
      return false;
    }
    const info = await youtube.getInfo(videoId);
    // Log basic video info
    console.log(`Downloading: ${info.basic_info.title} by ${info.basic_info.channel?.name || 'Unknown'}`);
    // Try to download using the built-in download method
    // const stream = await info.download({
    //   type: 'audio',
    //   quality: 'best'
    // });

    const stream = await youtube.download(videoId, {
      type: "audio",
      quality: "best",
      client: "YTMUSIC",
    });

    if (!stream) {
      console.warn('Failed to get download stream from YouTube.js');
      return false;
    }

    const dir = path.dirname(outPath);
    if (!fs.existsSync(dir)) fs.mkdirSync(dir, { recursive: true });

    const writeStream = fs.createWriteStream(outPath);
    let bytes = 0;

    return new Promise<boolean>((resolve, reject) => {
      const reader = stream.getReader();

      const pump = async () => {
        try {
          while (true) {
            const { done, value } = await reader.read();
            if (done) break;

            bytes += value.length;
            writeStream.write(value);
          }

          writeStream.end();

          // Validate minimum size for real audio
          if (bytes < 500 * 1024) {
            reject(new Error(`Downloaded file too small: ${bytes} bytes`));
            return;
          }

          resolve(true);
        } catch (e) {
          writeStream.destroy();
          reject(e);
        }
      };

      writeStream.on('error', reject);
      pump();
    });
  } catch (e) {
    console.warn('YouTube.js download error:', e);
    return false;
  }
}

async function validateAudioFile(filePath: string): Promise<boolean> {
  try {
    // Check file header to ensure it's actually audio content
    const buffer = await fs.promises.readFile(filePath, { start: 0, end: 1024 });
    const header = buffer.toString('binary', 0, 20);

    // Check for common audio file signatures
    const isWebM = header.includes('\x1a\x45\xdf\xa3'); // WebM/Matroska
    const isMP4 = header.includes('ftyp') || header.includes('mp4');
    const isOgg = header.includes('OggS'); // OGG/Opus files start with OggS
    const isWAV = header.includes('RIFF') && header.includes('WAVE');
    const isMP3 = buffer[0] === 0xFF && (buffer[1] & 0xE0) === 0xE0; // MP3 frame sync

    // Additional check for Opus in OGG container
    const isOpus = isOgg && (header.includes('OpusHead') || header.includes('OpusTags'));

    if (!isWebM && !isMP4 && !isOgg && !isWAV && !isMP3 && !isOpus) {
      console.warn("File doesn't appear to be valid audio format:", filePath);
      return false;
    }

    // Additional check: file should be at least 30 seconds worth of compressed audio
    const stats = await fs.promises.stat(filePath);
    const minExpectedSize = 500 * 1024; // ~500KB minimum for 30+ seconds of audio
    if (stats.size < minExpectedSize) {
      console.warn(`Audio file too small (${stats.size} bytes), expected at least ${minExpectedSize}:`, filePath);
      return false;
    }

    return true;
  } catch (e) {
    console.warn("Error validating audio file:", e);
    return false;
  }
}
