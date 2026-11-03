/**
 * Fixture songs for browser mode. Lyrics are original text written for this
 * fixture, not real songs.
 */
import type { LyricLine, Track } from "../../lib/types";

/** Build a line whose words are spread evenly across its duration. */
function line(time: number, duration: number, words: string[], sep = " "): LyricLine {
  const step = duration / words.length;
  return {
    time,
    duration,
    text: words.join(sep),
    words: words.map((text, i) => ({ time: time + i * step, duration: step * 0.92, text })),
  };
}

export interface MockSong {
  track: Track;
  lines: LyricLine[];
  translations: Record<string, string>;
}

export const englishSong: MockSong = {
  track: { title: "Paper Lanterns", artist: "The Quiet Hours", duration: 48 },
  lines: [
    line(2, 3.6, ["We", "hung", "paper", "lanterns", "on", "the", "fire", "escape"]),
    line(6, 3.4, ["and", "counted", "every", "train", "that", "didn't", "stop"]),
    line(10, 3.8, ["Your", "laugh", "was", "louder", "than", "the", "city"]),
    line(14, 3.2, ["so", "I", "kept", "it", "in", "my", "coat"]),
    line(18, 4, ["Hold", "the", "light", "a", "little", "longer"]),
    line(22.5, 4, ["till", "the", "morning", "finds", "us", "here"]),
    { time: 27, text: "", duration: 4 },
    line(31, 3.6, ["Every", "window", "hums", "a", "different", "song"]),
    line(35, 3.6, ["but", "ours", "is", "the", "one", "I", "know"]),
    line(39.5, 4.5, ["Hold", "the", "light", "a", "little", "longer"]),
  ],
  translations: {
    "we hung paper lanterns on the fire escape": "Colgamos faroles de papel en la escalera de incendios",
    "and counted every train that didn't stop": "y contamos cada tren que no se detuvo",
    "your laugh was louder than the city": "Tu risa sonaba más que la ciudad",
    "so i kept it in my coat": "así que la guardé en mi abrigo",
    "hold the light a little longer": "Sostén la luz un poco más",
    "till the morning finds us here": "hasta que la mañana nos encuentre aquí",
    "every window hums a different song": "Cada ventana tararea una canción distinta",
    "but ours is the one i know": "pero la nuestra es la que conozco",
  },
};

export const japaneseSong: MockSong = {
  track: { title: "夜明けの駅", artist: "Minato Line", duration: 30 },
  lines: [
    line(1.5, 4, ["夜明けの", "駅で", "君を", "待つ"], ""),
    line(6, 4, ["白い", "息が", "空に", "溶けていく"], ""),
    line(10.5, 4, ["遠い", "街の", "灯りを", "数えて"], ""),
    line(15, 4.5, ["明日の", "歌を", "口ずさむ"], ""),
    line(20, 4, ["夜明けの", "駅で", "君を", "待つ"], ""),
  ],
  translations: {
    "夜明けの駅で君を待つ": "Waiting for you at the station at dawn",
    "白い息が空に溶けていく": "My white breath melts into the sky",
    "遠い街の灯りを数えて": "Counting the lights of a distant town",
    "明日の歌を口ずさむ": "I hum tomorrow's song",
  },
};

/** Readings for the kanji used above (longest match wins). */
export const readings: Record<string, string> = {
  夜明: "よあ",
  駅: "えき",
  君: "きみ",
  待: "ま",
  白: "しろ",
  息: "いき",
  空: "そら",
  溶: "と",
  遠: "とお",
  街: "まち",
  灯: "あか",
  数: "かぞ",
  明日: "あした",
  歌: "うた",
  口: "くち",
};
