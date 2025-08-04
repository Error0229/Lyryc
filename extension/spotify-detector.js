// Spotify Web Player detector
class SpotifyDetector {
  constructor() {
    this.currentTrack = null;
    this.isPlaying = false;
    this.observer = null;
    this.init();
  }

  init() {
    // Wait for Spotify to load
    this.waitForSpotify().then(() => {
      this.startTracking();
    });
  }

  async waitForSpotify() {
    return new Promise((resolve) => {
      const checkForElements = () => {
        const nowPlayingBar = document.querySelector('[data-testid="now-playing-widget"]') ||
                            document.querySelector('.Root__now-playing-bar') ||
                            document.querySelector('.now-playing-bar');
        
        if (nowPlayingBar) {
          resolve();
        } else {
          setTimeout(checkForElements, 1000);
        }
      };
      checkForElements();
    });
  }

  startTracking() {
    // Set up mutation observer to watch for changes
    this.observer = new MutationObserver(() => {
      this.detectCurrentTrack();
    });

    // Observe the entire body for changes
    this.observer.observe(document.body, {
      childList: true,
      subtree: true,
      attributes: true,
      attributeFilter: ['aria-label', 'title']
    });

    // Initial detection
    this.detectCurrentTrack();
    
    // Set up periodic checks
    setInterval(() => {
      this.detectCurrentTrack();
    }, 2000);
  }

  detectCurrentTrack() {
    try {
      // Enhanced selectors for different Spotify layouts and updates
      const trackSelectors = [
        '[data-testid="now-playing-widget"] a[title]:first-of-type',
        '[data-testid="now-playing-widget"] .main-trackInfo-name a',
        '.Root__now-playing-bar [data-testid="context-item-link-track"]',
        '.now-playing-bar .track-info__name a',
        '.Root__now-playing-bar .Type__TypeElement-goli3j-0:first-child a',
        '[data-testid="now-playing-widget"] [dir="auto"]:first-of-type a',
        '.main-nowPlayingBar-trackInfo .main-trackInfo-name a',
        '.player-controls__left .track-info__name a'
      ];

      const artistSelectors = [
        '[data-testid="now-playing-widget"] a[title]:last-of-type',
        '[data-testid="now-playing-widget"] .main-trackInfo-artists a',
        '.Root__now-playing-bar [data-testid="context-item-link-artist"]',
        '.now-playing-bar .track-info__artists a',
        '.Root__now-playing-bar .Type__TypeElement-goli3j-0:last-child a',
        '[data-testid="now-playing-widget"] [dir="auto"]:last-of-type a',
        '.main-nowPlayingBar-trackInfo .main-trackInfo-artists a',
        '.player-controls__left .track-info__artists a'
      ];

      let trackName = null;
      let artistName = null;

      // Try to find track name
      for (const selector of trackSelectors) {
        const element = document.querySelector(selector);
        if (element) {
          trackName = element.textContent?.trim() || element.title?.trim();
          if (trackName) break;
        }
      }

      // Try to find artist name
      for (const selector of artistSelectors) {
        const element = document.querySelector(selector);
        if (element) {
          artistName = element.textContent?.trim() || element.title?.trim();
          if (artistName) break;
        }
      }

      // Alternative method: use document title
      if (!trackName || !artistName) {
        const title = document.title;
        if (title && title !== 'Spotify') {
          const parts = title.split(' • ');
          if (parts.length >= 2) {
            trackName = parts[0];
            artistName = parts[1];
          }
        }
      }

      // Enhanced play state detection
      const playButtons = [
        '[data-testid="control-button-playpause"]',
        '.player-controls__buttons .control-button--play-pause',
        '.main-playPauseButton-button'
      ];
      
      let isCurrentlyPlaying = false;
      for (const selector of playButtons) {
        const playButton = document.querySelector(selector);
        if (playButton) {
          const ariaLabel = playButton.getAttribute('aria-label') || '';
          const title = playButton.getAttribute('title') || '';
          isCurrentlyPlaying = ariaLabel.includes('Pause') || title.includes('Pause') || 
                             playButton.classList.contains('playing') ||
                             playButton.querySelector('.playing') !== null;
          if (isCurrentlyPlaying) break;
        }
      }

      // Get album art
      let thumbnail = null;
      const albumArt = document.querySelector('[data-testid="now-playing-widget"] img') ||
                      document.querySelector('.Root__now-playing-bar img') ||
                      document.querySelector('.now-playing-bar img');
      
      if (albumArt) {
        thumbnail = albumArt.src;
      }

      if (trackName && artistName) {
        const track = {
          title: trackName,
          artist: artistName,
          thumbnail: thumbnail,
          source: 'spotify',
          url: window.location.href,
          timestamp: Date.now()
        };

        // Only send update if track changed or play state changed
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

          console.log('Spotify track detected:', track, 'Playing:', isCurrentlyPlaying);
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
      console.error('Error detecting Spotify track:', error);
    }
  }

  destroy() {
    if (this.observer) {
      this.observer.disconnect();
    }
  }
}

// Initialize detector when page loads
if (document.readyState === 'loading') {
  document.addEventListener('DOMContentLoaded', () => {
    new SpotifyDetector();
  });
} else {
  new SpotifyDetector();
}