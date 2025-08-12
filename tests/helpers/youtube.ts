import * as path from "node:path";
import * as fs from "node:fs";
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

    const basePath = path.join(TEMP_DIR, name);

    // Try YouTube.js first (primary method)
    try {
      const downloadedPath = await downloadWithYouTubeJS(url, basePath);
      if (downloadedPath) {
        const isValid = await validateAudioFile(downloadedPath);
        if (isValid) return downloadedPath;
        try { fs.unlinkSync(downloadedPath); } catch { }
      }
    } catch (e) {
      console.warn('YouTube.js download failed:', e);
    }

    // Fallback to yt-dlp if available
    try {
      const hasYtDlp = await isCommandAvailable("yt-dlp");
      if (hasYtDlp) {
        const downloadedPath = await downloadWithYtDlp(url, basePath);
        if (downloadedPath) {
          const isValid = await validateAudioFile(downloadedPath);
          if (isValid) return downloadedPath;
          try { fs.unlinkSync(downloadedPath); } catch { }
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

async function downloadWithYtDlp(url: string, basePath: string): Promise<string | null> {
  return new Promise((resolve) => {
    const p = spawn("yt-dlp", [
      "-f", "ba", // best audio
      "-o", basePath + ".%(ext)s", // Let yt-dlp choose extension
      "--no-playlist",
      "--no-warnings",
      "-x", // extract audio
      "--audio-format", "best", // keep best audio format
      url,
    ], { stdio: "inherit" });

    p.on("error", () => resolve(null));
    p.on("exit", (code) => {
      if (code !== 0) {
        resolve(null);
        return;
      }

      // Check for output files with common audio extensions
      const extensions = ['.opus', '.m4a', '.webm', '.mp3', '.ogg'];
      for (const ext of extensions) {
        const possiblePath = basePath + ext;
        if (fs.existsSync(possiblePath)) {
          resolve(possiblePath);
          return;
        }
      }
      resolve(null);
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

async function downloadWithYouTubeJS(url: string, basePath: string): Promise<string | null> {
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
      return null;
    }
    const info = await youtube.getInfo(videoId);
    // Log basic video info
    console.log(`Downloading: ${info.basic_info.title} by ${info.basic_info.channel?.name || 'Unknown'}`);

    // Debug: Check available formats
    console.log('Available audio formats:', info.streaming_data?.adaptive_formats?.filter(f => f.has_audio && !f.has_video).map(f => ({
      itag: f.itag,
      mime_type: f.mime_type,
      bitrate: f.bitrate,
      url: !!f.url,
      cipher: !!f.cipher
    })));

    // Try alternative download approaches
    try {
      // Method 1: Try using info.download directly
      console.log('Trying info.download method...');
      const stream = await info.download({
        type: 'audio',
        quality: 'best'
      });

      if (stream) {
        console.log('Successfully got stream from info.download');
        return await writeStreamToFile(stream, basePath + '.webm');
      }
    } catch (e) {
      console.warn('info.download failed:', e.message);
    }

    try {
      // Method 2: Try using youtube.download
      console.log('Trying youtube.download method...');
      const stream = await youtube.download(videoId, {
        type: "audio",
        quality: "best",
      });

      if (stream) {
        console.log('Successfully got stream from youtube.download');
        return await writeStreamToFile(stream, basePath + '.webm');
      }
    } catch (e) {
      console.warn('youtube.download failed:', e.message);
    }

    try {
      // Method 3: Try manual format selection and download
      console.log('Trying manual format selection...');
      const audioFormats = info.streaming_data?.adaptive_formats?.filter(f => f.has_audio && !f.has_video) || [];

      // Try formats in order of preference: opus, m4a, webm
      const preferredFormats = ['opus', 'm4a', 'webm'];
      for (const formatType of preferredFormats) {
        const format = audioFormats.find(f => f.mime_type?.includes(formatType));
        if (format && format.url) {
          console.log(`Found ${formatType} format with direct URL, attempting download...`);
          try {
            const response = await fetch(format.url);
            if (response.ok && response.body) {
              console.log(`Successfully fetched ${formatType} stream`);
              const extension = formatType === 'opus' ? '.opus' : formatType === 'm4a' ? '.m4a' : '.webm';
              return await writeResponseToFile(response, basePath + extension);
            }
          } catch (e) {
            console.warn(`Failed to fetch ${formatType} format:`, e.message);
          }
        }
      }
    } catch (e) {
      console.warn('Manual format selection failed:', e.message);
    }

    console.warn('All YouTube.js download methods failed');
    return null;
  } catch (e) {
    console.warn('YouTube.js download error:', e);
    return null;
  }
}

async function writeStreamToFile(stream: ReadableStream, outPath: string): Promise<string | null> {
  const dir = path.dirname(outPath);
  if (!fs.existsSync(dir)) fs.mkdirSync(dir, { recursive: true });

  const writeStream = fs.createWriteStream(outPath);
  let bytes = 0;

  return new Promise<string | null>((resolve, reject) => {
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

        console.log(`Successfully wrote ${bytes} bytes to ${outPath}`);
        resolve(outPath);
      } catch (e) {
        writeStream.destroy();
        reject(e);
      }
    };

    writeStream.on('error', reject);
    pump();
  });
}

async function writeResponseToFile(response: Response, outPath: string): Promise<string | null> {
  const dir = path.dirname(outPath);
  if (!fs.existsSync(dir)) fs.mkdirSync(dir, { recursive: true });

  const writeStream = fs.createWriteStream(outPath);
  let bytes = 0;

  return new Promise<string | null>((resolve, reject) => {
    if (!response.body) {
      reject(new Error('No response body'));
      return;
    }

    const reader = response.body.getReader();

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

        console.log(`Successfully wrote ${bytes} bytes to ${outPath}`);
        resolve(outPath);
      } catch (e) {
        writeStream.destroy();
        reject(e);
      }
    };

    writeStream.on('error', reject);
    pump();
  });
}

async function validateAudioFile(filePath: string): Promise<boolean> {
  try {
    // Check file header to ensure it's actually audio content
    const fd = await fs.promises.open(filePath, 'r');
    const buffer = Buffer.alloc(1024);
    await fd.read(buffer, 0, 1024, 0);
    await fd.close();
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
