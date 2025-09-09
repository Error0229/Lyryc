declare module 'kuroshiro' {
  export interface ConvertOptions {
    to?: 'hiragana' | 'katakana' | 'romaji';
    mode?: 'spaced' | 'okurigana' | 'furigana' | 'normal';
  }

  export default class Kuroshiro {
    init(analyzer: any): Promise<void>;
    convert(text: string, options: ConvertOptions): Promise<string>;
  }
}

declare module 'kuroshiro-analyzer-kuromoji' {
  export default class KuromojiAnalyzer {
    constructor();
  }
}