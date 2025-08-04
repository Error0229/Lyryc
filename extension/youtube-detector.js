// YouTube Music detector
class YouTubeMusicDetector {
  constructor() {
    this.currentTrack = null;
    this.isPlaying = false;
    this.observer = null;
    this.init();
  }

  init() {
    // Wait for YouTube Music to load
    this.waitForYouTubeMusic().then(() => {
      this.startTracking();
    });
  }

  async waitForYouTubeMusic() {
    return new Promise((resolve) => {
      const checkForElements = () => {
        // Check for YouTube Music app container
        const ytMusicApp = document.querySelector('ytmusic-app') ||
                          document.querySelector('#main-panel') ||
                          document.querySelector('.style-scope.ytmusic-app-layout') ||
                          document.querySelector('ytmusic-player-bar') ||
                          document.querySelector('.player-bar') ||
                          document.querySelector('#layout .player-page');
        
        if (ytMusicApp) {
          console.log('YouTube Music detected, initializing...');
          resolve();
        } else {
          setTimeout(checkForElements, 1000);
        }
      };
      checkForElements();
    });
  }

  startTracking() {
    // Set up mutation observer
    this.observer = new MutationObserver(() => {
      this.detectCurrentTrack();
    });

    this.observer.observe(document.body, {
      childList: true,
      subtree: true,
      attributes: true,
      attributeFilter: ['aria-label', 'title', 'alt']
    });

    // Initial detection
    this.detectCurrentTrack();
    
    // Periodic checks
    setInterval(() => {
      this.detectCurrentTrack();
    }, 2000);
  }

  detectCurrentTrack() {
    try {
      let trackName = null;
      let artistName = null;
      let thumbnail = null;

      // YouTube Music uses various selectors across different layouts
      const trackSelectors = [
        // Player bar selectors
        'ytmusic-player-bar .content-info-wrapper .title',
        'ytmusic-player-bar .middle-controls-buttons .title', 
        'ytmusic-player-bar yt-formatted-string.title',
        '#layout ytmusic-player-bar .title',
        '.ytmusic-player-bar .title',
        
        // Alternative player selectors
        '.player-bar-middle-section .song-title',
        '.middle-controls .content-info-wrapper .title',
        '.content-info-wrapper .title a',
        
        // Modern YouTube Music selectors
        'ytmusic-player-bar .content-info-wrapper .title a',
        'ytmusic-player-bar .middle-controls .title',
        'ytmusic-player .song-title',
        
        // Legacy selectors
        '.content-info-wrapper .title',
        '.player-bar-wrapper .song-title',
        '#layout .player-page .content-info-wrapper .title yt-formatted-string',
        '.middle-controls-buttons ~ div .title',
        '.player-bar-wrapper .content-info-wrapper .title yt-formatted-string',
        
        // Fallback selectors
        '[class*="title"] a[href*="/watch"]',
        '.ytmusic-player-bar [class*="title"]',
        '#player-bar .title'
      ];

      const artistSelectors = [
        // Artist selectors corresponding to track selectors
        'ytmusic-player-bar .content-info-wrapper .byline',
        'ytmusic-player-bar .middle-controls-buttons .byline',
        'ytmusic-player-bar yt-formatted-string.byline',
        '#layout ytmusic-player-bar .byline', 
        '.ytmusic-player-bar .byline',
        
        // Alternative artist selectors
        '.player-bar-middle-section .song-artist',
        '.middle-controls .content-info-wrapper .byline',
        '.content-info-wrapper .byline a',
        
        // Modern YouTube Music artist selectors
        'ytmusic-player-bar .content-info-wrapper .byline a',
        'ytmusic-player-bar .middle-controls .byline',
        'ytmusic-player .song-artist',
        
        // Legacy selectors
        '.content-info-wrapper .subtitle a',
        'ytmusic-player-bar .subtitle',
        '.player-bar-wrapper .song-info .subtitle',
        '#layout .player-page .content-info-wrapper .subtitle a',
        '.ytmusic-player-bar .byline a',
        '.middle-controls-buttons ~ div .subtitle a',
        '.player-bar-wrapper .content-info-wrapper .subtitle a',
        
        // Fallback artist selectors
        '[class*="byline"] a[href*="/channel"]',
        '.ytmusic-player-bar [class*="byline"]',
        '#player-bar .byline'
      ];

      // Try to find track name
      for (const selector of trackSelectors) {
        const element = document.querySelector(selector);
        if (element) {
          trackName = element.textContent?.trim() || 
                     element.title?.trim() || 
                     element.getAttribute('aria-label')?.trim();
          if (trackName && trackName !== 'YouTube Music') {
            console.log(`Found track with selector: ${selector} -> ${trackName}`);
            break;
          }
        }
      }

      // Try to find artist name
      for (const selector of artistSelectors) {
        const element = document.querySelector(selector);
        if (element) {
          artistName = element.textContent?.trim() || 
                      element.title?.trim() || 
                      element.getAttribute('aria-label')?.trim();
          if (artistName && artistName !== 'YouTube Music') {
            console.log(`Found artist with selector: ${selector} -> ${artistName}`);
            break;
          }
        }
      }

      // Alternative method: use document title
      if (!trackName || !artistName) {
        const title = document.title;
        if (title && title !== 'YouTube Music' && !title.includes('YouTube')) {
          // YouTube Music title format: "Song - Artist - YouTube Music"
          const parts = title.split(' - ');
          if (parts.length >= 2) {
            trackName = trackName || parts[0]?.trim();
            artistName = artistName || parts[1]?.trim();
            console.log(`Found track from title: ${trackName} by ${artistName}`);
          }
        }
      }

      // Alternative method: use media session API
      if ((!trackName || !artistName) && 'mediaSession' in navigator && navigator.mediaSession.metadata) {
        const metadata = navigator.mediaSession.metadata;
        trackName = trackName || metadata.title;
        artistName = artistName || metadata.artist;
        console.log(`Found track from mediaSession: ${trackName} by ${artistName}`);
      }

      // Play state detection - YouTube Music play/pause button
      const playButtonSelectors = [
        'ytmusic-player-bar #play-pause-button',
        'ytmusic-player-bar .play-pause-button',
        '#player-bar-middle .play-pause-button',
        '.middle-controls .play-pause-button',
        'ytmusic-player .play-pause-button',
        '[aria-label*="pause" i][role="button"]',
        '[aria-label*="play" i][role="button"]',
        'button[data-title-no-tooltip*="pause" i]',
        'button[data-title-no-tooltip*="play" i]',
        '#play-pause-button',
        '.play-pause-button',
        '.middle-controls-buttons .play-pause-button'
      ];
      
      let isCurrentlyPlaying = false;
      for (const selector of playButtonSelectors) {
        const playButton = document.querySelector(selector);
        if (playButton) {
          const ariaLabel = playButton.getAttribute('aria-label') || '';
          const title = playButton.getAttribute('title') || playButton.getAttribute('data-title-no-tooltip') || '';
          const tooltip = playButton.querySelector('[role="tooltip"]')?.textContent || '';
          
          // If button says "Pause", music is playing
          isCurrentlyPlaying = ariaLabel.toLowerCase().includes('pause') || 
                             title.toLowerCase().includes('pause') ||
                             tooltip.toLowerCase().includes('pause') ||
                             playButton.querySelector('[aria-label*="pause" i]') !== null ||
                             playButton.classList.contains('playing') ||
                             document.querySelector('.playing') !== null;
          
          if (isCurrentlyPlaying) {
            console.log(`Music is playing (detected via ${selector})`);
            break;
          }
        }
      }

      // Alternative play state detection using media session
      if (!isCurrentlyPlaying && 'mediaSession' in navigator) {
        // Check if media session indicates playing state
        try {
          // This is a fallback - we can't directly check mediaSession playback state
          // but we can infer from the presence of metadata
          isCurrentlyPlaying = navigator.mediaSession.metadata !== null;
        } catch (e) {
          // Ignore errors
        }
      }

      // Get thumbnail
      const thumbnailSelectors = [
        '.player-bar-wrapper .song-image img',
        'ytmusic-player-bar .image img',
        '#layout .player-page .song-image img',
        '.content-info-wrapper img'
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
          source: 'youtube-music',
          url: window.location.href,
          timestamp: Date.now()
        };

        // Only send if changed
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

          console.log('YouTube Music track detected:', track, 'Playing:', isCurrentlyPlaying);
        }
      } else if (this.currentTrack) {
        // Track stopped
        this.currentTrack = null;
        this.isPlaying = false;
        chrome.runtime.sendMessage({
          type: 'TRACK_STOPPED'
        });
      }

    } catch (error) {
      console.error('Error detecting YouTube Music track:', error);
    }
  }

  destroy() {
    if (this.observer) {
      this.observer.disconnect();
    }
  }
}

// Initialize detector
if (document.readyState === 'loading') {
  document.addEventListener('DOMContentLoaded', () => {
    new YouTubeMusicDetector();
  });
} else {
  new YouTubeMusicDetector();
}