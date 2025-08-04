# 音樂歌詞同步顯示系統 - 技術規格

## 專案概述

一個跨平台桌面應用程式，能夠即時顯示瀏覽器中正在播放音樂的歌詞，並提供字級同步高亮效果。這是一個學習用途的個人專案，專注於免費API和開源技術的整合。

## 系統架構

```
Browser Music → Extension → Desktop App → Lyrics Display
     ↓              ↓           ↓            ↓
  Spotify/YT    Track Info   Processing   Sync Effects
```

## 一、技術棧選擇

### 核心框架
- **桌面應用**: Tauri + React + TypeScript
- **樣式系統**: Tailwind CSS
- **動畫庫**: Framer Motion
- **狀態管理**: Zustand

### API 服務
- **歌詞來源**: LRCLIB (免費)
- **音樂識別**: 瀏覽器擴展檢測
- **時間戳對齊**: 自開發 AI 模型

## 二、系統組件

### 2.1 瀏覽器擴展 (Browser Extension)

**功能**：
- 檢測當前播放的音樂信息
- 支援 Spotify Web Player、YouTube Music
- 即時傳送歌曲信息到桌面應用

**技術實現**：
```javascript
// Chrome Extension Manifest V3
{
  "manifest_version": 3,
  "permissions": ["activeTab", "storage"],
  "content_scripts": [{
    "matches": ["*://open.spotify.com/*", "*://music.youtube.com/*"],
    "js": ["content.js"]
  }]
}
```

### 2.2 桌面應用核心 (Tauri App)

**架構**：
```
src/
├── components/          # React 組件
│   ├── LyricsDisplay/   # 歌詞顯示組件
│   ├── Settings/        # 設定面板
│   └── Overlay/         # 浮動視窗
├── services/            # 服務層
│   ├── lrclib.ts       # LRCLIB API
│   ├── extension.ts    # 擴展通信
│   └── alignment.ts    # 時間戳對齊
├── utils/              # 工具函數
└── stores/             # 狀態管理
```

### 2.3 歌詞服務系統

**LRCLIB API 整合**：
```typescript
interface LyricsResponse {
  id: number;
  trackName: string;
  artistName: string;
  plainLyrics: string;
  syncedLyrics: string; // LRC format
}
```

**本地快取策略**：
- 使用 IndexedDB 快取歌詞數據
- 快取時間：7天
- 快取大小限制：100MB

## 三、核心功能實現

### 3.1 音樂檢測與通信

**Extension → Desktop 通信**：
```javascript
// Extension Content Script
const trackInfo = {
  title: document.querySelector('[data-testid="now-playing-widget"] span').textContent,
  artist: document.querySelector('[data-testid="now-playing-widget"] a').textContent,
  timestamp: Date.now()
};

// Send to desktop app via WebSocket/Native Messaging
```

### 3.2 歌詞獲取與解析

**LRCLIB API 調用**：
```typescript
async function fetchLyrics(trackName: string, artistName: string) {
  const response = await fetch(
    `https://lrclib.net/api/search?track_name=${trackName}&artist_name=${artistName}`
  );
  return response.json();
}
```

**LRC 格式解析**：
```typescript
interface LyricLine {
  time: number; // milliseconds
  text: string;
  words?: WordTiming[];
}

interface WordTiming {
  start: number;
  end: number;
  word: string;
}
```

### 3.3 AI 時間戳對齊系統 ✅ IMPLEMENTED

**多語言智能對齊方案**：
- 動態時間彎曲 (DTW) 算法
- Web Audio API + MFCC 特徵提取  
- 10+ 語言音素映射支援
- 字級時間戳自動生成
- 信心度評分與品質保證

**基本對齊算法**：
```typescript
function alignLyrics(audioData: AudioBuffer, lyrics: LyricLine[]) {
  // 1. 音頻特徵提取
  const features = extractMFCC(audioData);
  
  // 2. 文本特徵提取
  const textFeatures = extractTextFeatures(lyrics);
  
  // 3. DTW 對齊
  const alignment = dtw(features, textFeatures);
  
  return alignment;
}
```

### 3.4 歌詞顯示組件

**React 組件結構**：
```typescript
interface LyricsDisplayProps {
  lyrics: LyricLine[];
  currentTime: number;
  config: DisplayConfig;
}

interface DisplayConfig {
  fontSize: number;
  fontFamily: string;
  primaryColor: string;
  highlightColor: string;
  animationType: 'slide' | 'fade' | 'wave';
}
```

**同步動畫效果**：
```typescript
// 使用 Framer Motion
const currentLine = lyrics.find(line => 
  currentTime >= line.time && currentTime < line.time + line.duration
);

<motion.div
  initial={{ opacity: 0, y: 20 }}
  animate={{ opacity: 1, y: 0 }}
  key={currentLine?.id}
>
  {currentLine?.text}
</motion.div>
```

## 四、使用者介面設計

### 4.1 主要顯示模式

**浮動視窗 (Overlay)**：
- 桌面最上層顯示
- 可拖拽定位
- 透明背景支援
- 一鍵隱藏/顯示

**設定面板**：
- 樣式自訂
- 快捷鍵設定
- 資料來源選擇
- 同步精度調整

### 4.2 主題系統

**預設主題**：
1. **經典** - 白底黑字
2. **暗夜** - 黑底白字  
3. **霓虹** - 彩色漸變
4. **卡拉OK** - 金色高亮

**自訂選項**：
- 字體大小 (12-72px)
- 字體家族 (系統字體)
- 顏色配置 (RGB/HEX)
- 透明度 (0-100%)
- 動畫速度 (0.5x-3x)

## 五、開發階段規劃

### Phase 1: 基礎功能 (2-3週) ✅ COMPLETED
- [x] Tauri 專案初始化
- [x] Chrome Extension 開發 (支援 Spotify, YouTube Music, Apple Music, SoundCloud)
- [x] LRCLIB API 整合 (增強錯誤處理與重試機制)  
- [x] 基本歌詞顯示 (支援 LRC 格式與字級時間戳)
- [x] Extension ↔ Desktop 通信 (WebSocket 實現)

### Phase 2: 增強功能 (2-3週) ✅ COMPLETED
- [x] 本地快取系統 (IndexedDB 實現，支援 50MB 容量)
- [x] 多主題支援 (4個預設主題：Classic, Dark Night, Neon Glow, Karaoke Gold)
- [x] 同步動畫效果 (Framer Motion 實現)
- [ ] 設定面板UI
- [ ] 快捷鍵支援

### Phase 3: AI對齊 (2-4週) ✅ MOSTLY COMPLETED
- [x] 音頻分析模組 (Web Audio API + MFCC 特徵提取)
- [x] 開源對齊算法 (Dynamic Time Warping + 多語言音素映射)
- [x] 字級時間戳 (智能生成與語言特定優化)
- [x] 精確度優化 (信心度評分與多重策略搜尋)

### Phase 4: 完善與優化 (1-2週) 🚧 IN PROGRESS
- [ ] 效能優化
- [ ] 錯誤處理
- [ ] 浮動視窗模式
- [ ] 用戶體驗改進
- [ ] 文檔完善

## 六、技術挑戰與解決方案

### 6.1 瀏覽器安全限制
**問題**: Content Security Policy 限制
**解決**: Native Messaging API + Local WebSocket

### 6.2 歌詞數據不完整
**問題**: LRCLIB 覆蓋率有限
**解決**: 多資料源整合 + 使用者貢獻

### 6.3 時間同步精確度
**問題**: 瀏覽器播放器延遲
**解決**: 動態延遲補償 + 使用者微調

### 6.4 跨平台相容性
**問題**: 不同作業系統行為差異
**解決**: Tauri 原生API + 條件式實現

## 七、檔案結構

```
lyryc/
├── src-tauri/           # Rust 後端
│   ├── src/
│   │   ├── main.rs
│   │   ├── commands.rs  # Tauri 命令
│   │   └── lyrics.rs    # 歌詞處理
│   └── Cargo.toml
├── src/                 # React 前端
│   ├── components/
│   ├── services/
│   ├── stores/
│   ├── utils/
│   └── App.tsx
├── extension/           # Chrome Extension
│   ├── manifest.json
│   ├── content.js
│   ├── background.js
│   └── popup/
├── public/
└── package.json
```

## 八、部署與發佈

### 開發環境
```bash
# 安裝依賴
npm install

# 開發模式
npm run tauri dev

# 打包應用
npm run tauri build
```

### 發佈渠道
- GitHub Releases
- 自動更新系統 (Tauri Updater)
- Chrome Web Store (Extension)

---

**專案目標**: 打造一個輕量、美觀、功能完整的音樂歌詞同步顯示工具，作為學習 Tauri、React 和音頻處理技術的實踐專案。

## 實現狀態總結 (Implementation Status)

### ✅ 已完成功能 (Completed Features)
- **多平台音樂檢測**: 支援 Spotify, YouTube Music, Apple Music, SoundCloud
- **增強型 LRCLIB 整合**: 多重搜尋策略、錯誤處理、重試機制
- **WebSocket 即時通信**: Extension 與桌面應用的雙向通信
- **智能歌詞處理**: LRC 解析、字級時間戳、多語言支援
- **IndexedDB 快取系統**: 50MB 容量、7天有效期、自動清理
- **4 種預設主題**: Classic, Dark Night, Neon Glow, Karaoke Gold
- **AI 對齊算法**: DTW + MFCC + 多語言音素映射
- **動態動畫效果**: Framer Motion 驅動的流暢轉換

### 🚧 開發中功能 (In Progress)
- 設定面板 UI
- 浮動視窗模式
- 效能優化

### 📋 待實現功能 (Pending)
- 快捷鍵支援
- 綜合錯誤處理
- 用戶體驗改進

**技術亮點**:
- 🎨 **多語言 AI 對齊**: 支援 10+ 語言的智能歌詞同步
- 🎵 **字級精確度**: 單字層級的時間戳生成
- 🌈 **主題系統**: 完全可定制的視覺主題
- 💾 **智能快取**: 高效的本地存儲與管理
- 🔄 **即時同步**: WebSocket 實現的低延遲通信