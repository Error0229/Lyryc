// SoundCloud detector
class SoundCloudDetector {
  constructor() {
    this.currentTrack = null;
    this.isPlaying = false;
    this.observer = null;
    this.mediaListenersAttached = false;
    this.init();
  }

  init() {
    this.waitForSoundCloud().then(() => {
      this.startTracking();
    });
  }

  async waitForSoundCloud() {
    return new Promise((resolve) => {
      const checkForElements = () => {
        const playerBar = document.querySelector('.playControls') ||
                         document.querySelector('.playbackSoundBadge') ||
                         document.querySelector('.playbackTitle');
        
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
      attributeFilter: ['title', 'aria-label']
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

      // SoundCloud selectors
      const trackSelectors = [
        '.playbackSoundBadge__titleLink',
        '.playbackTitle__link',
        '.playControls__soundBadge .playbackSoundBadge__titleLink',
        '.playbackTitle a',
        '.soundTitle__title'
      ];

      const artistSelectors = [
        '.playbackSoundBadge__lightLink',
        '.playbackTitle__usernameLink',
        '.playControls__soundBadge .playbackSoundBadge__lightLink',
        '.playbackTitle .sc-link-light',
        '.soundTitle__username'
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
        if (title && title !== 'SoundCloud') {
          const parts = title.split(' by ');
          if (parts.length >= 2) {
            trackName = parts[0].trim();
            artistName = parts[1].replace(' | SoundCloud', '').trim();
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
        '.playbackSoundBadge .image__lightOutline img',
        '.playbackTitle .image img',
        '.playControls__soundBadge .image img',
        '.sound__artwork img'
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
          source: 'soundcloud',
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

          console.log('SoundCloud track detected:', track, 'Playing:', isCurrentlyPlaying);
        }
      } else if (this.currentTrack) {
        this.currentTrack = null;
        this.isPlaying = false;
        chrome.runtime.sendMessage({
          type: 'TRACK_STOPPED'
        });
      }

    } catch (error) {
      console.error('Error detecting SoundCloud track:', error);
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
            '.skipControl__next',
            '.playControls__next'
          ];
          for (const sel of selectors) { const btn = document.querySelector(sel); if (btn) { btn.click(); sendResponse({ success: true }); return true; } }
          break;
        }
        case 'previous': {
          const selectors = [
            '.skipControl__previous',
            '.playControls__prev'
          ];
          for (const sel of selectors) { const btn = document.querySelector(sel); if (btn) { btn.click(); sendResponse({ success: true }); return true; } }
          break;
        }
        case 'seek':
          if (mediaEl && typeof message.seekTime === 'number') { mediaEl.currentTime = message.seekTime; sendResponse({ success: true }); return true; }
          break;
      }
    } catch (e) { console.log('SoundCloud PLAYBACK_COMMAND failed', e); }
    sendResponse({ success: false });
  }
  return true;
});

// Initialize detector
if (document.readyState === 'loading') {
  document.addEventListener('DOMContentLoaded', () => {
    new SoundCloudDetector();
  });
} else {
  new SoundCloudDetector();
}
