// Spotify Web Player detector
class SpotifyDetector {
  constructor() {
    this.currentTrack = null;
    this.isPlaying = false;
    this.observer = null;
    this.lastProgressUpdate = 0;
    this.trackDuration = 0;
    this.currentProgress = 0;
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

      // Get progress and duration information
      const progressBar = document.querySelector('[data-testid="progress-bar"]') ||
                         document.querySelector('.playback-bar__progress-time') ||
                         document.querySelector('.progress-bar');
      
      let currentTime = 0;
      let duration = 0;
      
      if (progressBar) {
        // Try to get time from text elements
        const timeElements = document.querySelectorAll('[data-testid="progress-bar"] span');
        if (timeElements.length >= 2) {
          const currentTimeText = timeElements[0].textContent;
          const durationText = timeElements[1].textContent;
          
          currentTime = this.parseTimeString(currentTimeText);
          duration = this.parseTimeString(durationText);
        }
        
        // Fallback: try to get from progress bar itself
        if (duration === 0) {
          const progressElement = progressBar.querySelector('[aria-valuenow]');
          if (progressElement) {
            const valueNow = parseInt(progressElement.getAttribute('aria-valuenow'));
            const valueMax = parseInt(progressElement.getAttribute('aria-valuemax'));
            if (valueMax > 0) {
              currentTime = valueNow;
              duration = valueMax;
            }
          }
        }
      }
      
      this.currentProgress = currentTime;
      this.trackDuration = duration;

      if (trackName && artistName) {
        const track = {
          title: trackName,
          artist: artistName,
          thumbnail: thumbnail,
          source: 'spotify',
          url: window.location.href,
          timestamp: Date.now()
        };

        // Check if track is near end (within 3 seconds) and potentially auto-playing next
        const isNearEnd = duration > 0 && currentTime > 0 && (duration - currentTime) <= 3;
        const progressChanged = Math.abs(this.lastProgressUpdate - currentTime) > 1;
        
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
              data: { ...track, currentTime, duration }
            });
          } else {
            chrome.runtime.sendMessage({
              type: 'TRACK_PAUSED',
              data: { ...track, currentTime, duration }
            });
          }

          console.log('Spotify track detected:', track, 'Playing:', isCurrentlyPlaying);
        }
        // Send progress updates if significant change
        else if (progressChanged && isCurrentlyPlaying) {
          chrome.runtime.sendMessage({
            type: 'TRACK_PROGRESS',
            data: { ...track, currentTime, duration }
          });
        }
        
        this.lastProgressUpdate = currentTime;
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

  parseTimeString(timeStr) {
    if (!timeStr || typeof timeStr !== 'string') return 0;
    
    const parts = timeStr.split(':').map(p => parseInt(p.trim()));
    if (parts.length === 2) {
      return parts[0] * 60 + parts[1]; // minutes:seconds
    } else if (parts.length === 3) {
      return parts[0] * 3600 + parts[1] * 60 + parts[2]; // hours:minutes:seconds
    }
    return 0;
  }

  // Handle playback commands from desktop app
  handlePlaybackCommand(command, seekTime) {
    try {
      switch (command) {
        case 'play':
          this.clickPlayButton(false); // false = play
          break;
        case 'pause':
          this.clickPlayButton(true); // true = pause
          break;
        case 'seek':
          this.seekToTime(seekTime);
          break;
        default:
          console.log('Unknown playback command:', command);
      }
    } catch (error) {
      console.error('Error handling playback command:', error);
    }
  }

  clickPlayButton(shouldPause) {
    const playButtons = [
      '[data-testid="control-button-playpause"]',
      '.player-controls__buttons .control-button--play-pause',
      '.main-playPauseButton-button'
    ];
    
    for (const selector of playButtons) {
      const playButton = document.querySelector(selector);
      if (playButton) {
        const ariaLabel = playButton.getAttribute('aria-label') || '';
        const title = playButton.getAttribute('title') || '';
        const currentlyPlaying = ariaLabel.includes('Pause') || title.includes('Pause');
        
        // Only click if state needs to change
        if ((shouldPause && currentlyPlaying) || (!shouldPause && !currentlyPlaying)) {
          playButton.click();
          console.log(`${shouldPause ? 'Paused' : 'Played'} via button click`);
          return true;
        }
      }
    }
    return false;
  }

  seekToTime(timeInSeconds) {
    // Try to find progress bar and simulate click at desired position
    const progressBar = document.querySelector('[data-testid="progress-bar"]') ||
                       document.querySelector('.playback-bar__progress-time') ||
                       document.querySelector('.progress-bar');
    
    if (progressBar && this.trackDuration > 0) {
      const progressElement = progressBar.querySelector('[role="slider"]') ||
                             progressBar.querySelector('.progress-bar__slider') ||
                             progressBar;
      
      if (progressElement) {
        const rect = progressElement.getBoundingClientRect();
        const percentage = timeInSeconds / this.trackDuration;
        const clickX = rect.left + (rect.width * percentage);
        
        // Create and dispatch click event
        const clickEvent = new MouseEvent('click', {
          bubbles: true,
          cancelable: true,
          clientX: clickX,
          clientY: rect.top + rect.height / 2
        });
        
        progressElement.dispatchEvent(clickEvent);
        console.log(`Seeked to ${timeInSeconds}s (${(percentage * 100).toFixed(1)}%)`);
        return true;
      }
    }
    
    console.log('Could not find progress bar for seeking');
    return false;
  }

  destroy() {
    if (this.observer) {
      this.observer.disconnect();
    }
  }
}

// Initialize detector when page loads
let spotifyDetector = null;

if (document.readyState === 'loading') {
  document.addEventListener('DOMContentLoaded', () => {
    spotifyDetector = new SpotifyDetector();
  });
} else {
  spotifyDetector = new SpotifyDetector();
}

// Listen for playback commands from background script
chrome.runtime.onMessage.addListener((message, sender, sendResponse) => {
  if (message.type === 'PLAYBACK_COMMAND' && spotifyDetector) {
    spotifyDetector.handlePlaybackCommand(message.command, message.seekTime);
    sendResponse({ success: true });
  }
  return true;
});