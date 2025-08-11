// Apple Music Web detector
class AppleMusicDetector {
  constructor() {
    this.currentTrack = null;
    this.isPlaying = false;
    this.observer = null;
    this.mediaListenersAttached = false;
    this.init();
  }

  init() {
    this.waitForAppleMusic().then(() => {
      this.startTracking();
    });
  }

  async waitForAppleMusic() {
    return new Promise((resolve) => {
      const checkForElements = () => {
        const playerBar = document.querySelector('.web-chrome-playback-controls') ||
                         document.querySelector('.playback-controls') ||
                         document.querySelector('.now-playing-container');
        
        if (playerBar) {
          resolve();
        } else {
          setTimeout(checkForElements, 1000);
        }
      };
      checkForElements();
    });
  }

  startTracking() {
    this.observer = new MutationObserver(() => {
      this.detectCurrentTrack();
      this.attachMediaListeners();
    });

    this.observer.observe(document.body, {
      childList: true,
      subtree: true,
      attributes: true,
      attributeFilter: ['title', 'aria-label', 'alt']
    });

    this.detectCurrentTrack();
    this.attachMediaListeners();
    
    setInterval(() => {
      this.detectCurrentTrack();
    }, 2000);
  }

  detectCurrentTrack() {
    try {
      let trackName = null;
      let artistName = null;
      let thumbnail = null;

      // Apple Music selectors
      const trackSelectors = [
        '.web-chrome-playback-lcd__song-name',
        '.playback-controls .song-name',
        '.now-playing-container .song-name',
        '.web-chrome-playback-lcd .song-name-link',
        '.playback-controls__stack .song-name'
      ];

      const artistSelectors = [
        '.web-chrome-playback-lcd__sub-copy a',
        '.playback-controls .song-artist',
        '.now-playing-container .song-artist',
        '.web-chrome-playback-lcd .artist-name-link',
        '.playback-controls__stack .song-artist'
      ];

      // Find track name
      for (const selector of trackSelectors) {
        const element = document.querySelector(selector);
        if (element) {
          trackName = element.textContent?.trim() || element.title?.trim();
          if (trackName) break;
        }
      }

      // Find artist name
      for (const selector of artistSelectors) {
        const element = document.querySelector(selector);
        if (element) {
          artistName = element.textContent?.trim() || element.title?.trim();
          if (artistName) break;
        }
      }

      // Alternative: parse from document title
      if (!trackName || !artistName) {
        const title = document.title;
        if (title && !title.includes('Apple Music')) {
          const parts = title.split(' — ');
          if (parts.length >= 2) {
            trackName = parts[0].trim();
            artistName = parts[1].replace(' — Apple Music', '').trim();
          }
        }
      }

      // Language-agnostic: prefer media element state
      let isCurrentlyPlaying = false;
      const mediaEl = document.querySelector('audio, video');
      if (mediaEl) {
        try { isCurrentlyPlaying = !mediaEl.paused && !mediaEl.ended; } catch (_) {}
      }

      // Get thumbnail
      const thumbnailSelectors = [
        '.web-chrome-playback-lcd__artwork img',
        '.playback-controls .artwork img',
        '.now-playing-container .artwork img'
      ];

      for (const selector of thumbnailSelectors) {
        const img = document.querySelector(selector);
        if (img && img.src) {
          thumbnail = img.src;
          break;
        }
      }

      if (trackName && artistName) {
        const track = {
          title: trackName,
          artist: artistName,
          thumbnail: thumbnail,
          source: 'apple-music',
          url: window.location.href,
          timestamp: Date.now()
        };

        if (!this.currentTrack || 
            this.currentTrack.title !== track.title || 
            this.currentTrack.artist !== track.artist ||
            this.isPlaying !== isCurrentlyPlaying) {
          
          this.currentTrack = track;
          this.isPlaying = isCurrentlyPlaying;

          if (isCurrentlyPlaying) {
            chrome.runtime.sendMessage({
              type: 'TRACK_DETECTED',
              data: track
            });
          } else {
            chrome.runtime.sendMessage({
              type: 'TRACK_PAUSED',
              data: track
            });
          }

          console.log('Apple Music track detected:', track, 'Playing:', isCurrentlyPlaying);
        }
      } else if (this.currentTrack) {
        this.currentTrack = null;
        this.isPlaying = false;
        chrome.runtime.sendMessage({
          type: 'TRACK_STOPPED'
        });
      }

    } catch (error) {
      console.error('Error detecting Apple Music track:', error);
    }
  }

  attachMediaListeners() {
    if (this.mediaListenersAttached) return;
    const mediaEl = document.querySelector('audio, video');
    if (!mediaEl) return;
    this.mediaListenersAttached = true;

    mediaEl.addEventListener('play', () => {
      this.isPlaying = true;
      if (this.currentTrack) {
        chrome.runtime.sendMessage({ type: 'TRACK_DETECTED', data: this.currentTrack });
      }
    });

    mediaEl.addEventListener('pause', () => {
      this.isPlaying = false;
      if (this.currentTrack) {
        chrome.runtime.sendMessage({ type: 'TRACK_PAUSED', data: this.currentTrack });
      }
    });

    mediaEl.addEventListener('ended', () => {
      this.isPlaying = false;
      chrome.runtime.sendMessage({ type: 'TRACK_STOPPED' });
    });
  }

  destroy() {
    if (this.observer) {
      this.observer.disconnect();
    }
  }
}

// Listen for playback commands from background script
chrome.runtime.onMessage.addListener((message, sender, sendResponse) => {
  if (message.type === 'PLAYBACK_COMMAND') {
    try {
      const mediaEl = document.querySelector('audio, video');
      switch (message.command) {
        case 'play':
          if (mediaEl && mediaEl.paused) { mediaEl.play(); sendResponse({ success: true }); return true; }
          break;
        case 'pause':
          if (mediaEl && !mediaEl.paused) { mediaEl.pause(); sendResponse({ success: true }); return true; }
          break;
        case 'next': {
          const selectors = [
            '.web-chrome-playback-controls__next',
            '.next-button',
          ];
          for (const sel of selectors) { const btn = document.querySelector(sel); if (btn) { btn.click(); sendResponse({ success: true }); return true; } }
          break;
        }
        case 'previous': {
          const selectors = [
            '.web-chrome-playback-controls__previous',
            '.previous-button',
          ];
          for (const sel of selectors) { const btn = document.querySelector(sel); if (btn) { btn.click(); sendResponse({ success: true }); return true; } }
          break;
        }
        case 'seek':
          if (mediaEl && typeof message.seekTime === 'number') { mediaEl.currentTime = message.seekTime; sendResponse({ success: true }); return true; }
          break;
      }
    } catch (e) { console.log('AppleMusic PLAYBACK_COMMAND failed', e); }
    sendResponse({ success: false });
  }
  return true;
});

// Initialize detector
if (document.readyState === 'loading') {
  document.addEventListener('DOMContentLoaded', () => {
    new AppleMusicDetector();
  });
} else {
  new AppleMusicDetector();
}
