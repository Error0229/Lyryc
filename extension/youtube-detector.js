// YouTube Music detector
class YouTubeMusicDetector {
  constructor() {
    this.currentTrack = null;
    this.isPlaying = false;
    this.observer = null;
    this.timeUpdateInterval = null;
    this.videoListenersAttached = false;
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
      this.attachVideoListeners();
    });

    // Only observe player bar area instead of entire document
    const playerBar = /* document.querySelector('ytmusic-player-bar') || */ document.body;
    this.observer.observe(playerBar, {
      childList: true,
      subtree: true,
      attributes: true,
      attributeFilter: ['aria-label', 'title']
    });

    // Initial detection and listeners
    this.detectCurrentTrack();
    this.attachVideoListeners();

    // Periodic checks - reduced frequency
    setInterval(() => {
      this.detectCurrentTrack();
    }, 5000);
  }

  detectCurrentTrack() {
    try {
      let trackName = null;
      let artistName = null;
      let thumbnail = null;

      // YouTube Music uses various selectors across different layouts
      const trackSelectors = [
        // Primary selectors (most reliable)
        'ytmusic-player-bar .content-info-wrapper .title',
        'ytmusic-player-bar yt-formatted-string.title',
        '.content-info-wrapper .title a',
        '.ytmusic-player-bar .title',

        // Fallback selectors
        '.middle-controls .content-info-wrapper .title',
        '#layout ytmusic-player-bar .title'
      ];

      const artistSelectors = [
        // Primary artist selectors
        'ytmusic-player-bar .content-info-wrapper .byline',
        'ytmusic-player-bar yt-formatted-string.byline',
        '.content-info-wrapper .byline a',
        '.ytmusic-player-bar .byline',

        // Fallback selectors
        '.middle-controls .content-info-wrapper .byline',
        '#layout ytmusic-player-bar .byline'
      ];

      // Try to find track name
      for (const selector of trackSelectors) {
        const element = document.querySelector(selector);
        if (element) {
          trackName = element.textContent?.trim() ||
            element.title?.trim() ||
            element.getAttribute('aria-label')?.trim();
          // clean up track name
          // 1. if it contains 【*】 remove it
          trackName = trackName?.replace(/【.*?】/g, '').trim();
          // 2. if it contains " - YouTube Music" remove it
          trackName = trackName?.replace(/ - YouTube Music$/, '').trim();
          // 3. if it contains " - YouTube" remove it
          trackName = trackName?.replace(/ - YouTube$/, '').trim();
          // 4. remove everything after first '/', '-', or '｜' (covers, artists, etc)
          trackName = trackName?.split(/[/-｜]/)[0].trim();
          if (trackName && trackName !== 'YouTube Music') {
            // console.log(`Found track with selector: ${selector} -> ${trackName}`);
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
          // if the string in the format of "Artist • Album • Year" is found, we take the first part as artist
          if (artistName && artistName.includes('•')) {
            artistName = artistName.split('•')[0].trim();
          }
          if (artistName && artistName !== 'YouTube Music') {
            // console.log(`Found artist with selector: ${selector} -> ${artistName}`);
            break;
          }
        }
      }


      // Alternative method: use media session API
      if ((!trackName || !artistName) && 'mediaSession' in navigator && navigator.mediaSession.metadata) {
        const metadata = navigator.mediaSession.metadata;
        trackName = trackName || metadata.title;
        artistName = artistName || metadata.artist;
        // console.log(`Found track from mediaSession: ${trackName} by ${artistName}`);
      }

      // Play state detection - YouTube Music play/pause button
      const playButtonSelectors = [
        'ytmusic-player-bar #play-pause-button',
        'ytmusic-player-bar .play-pause-button',
        '.middle-controls .play-pause-button',
        '#play-pause-button'
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
            // console.log(`Music is playing (detected via ${selector})`);
            break;
          }
        }
      }

      // Do not infer playing state from mediaSession metadata; it's present even when paused

      // Get thumbnail
      const thumbnailSelectors = [
        'ytmusic-player-bar .image img',
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
            // Start sending time updates
            this.startTimeUpdates();
          } else {
            chrome.runtime.sendMessage({
              type: 'TRACK_PAUSED',
              data: track
            });
            // Stop sending time updates
            this.stopTimeUpdates();
          }

          console.log('YouTube Music track detected:', track, 'Playing:', isCurrentlyPlaying);
        }
      } else if (this.currentTrack) {
        // Track stopped
        this.currentTrack = null;
        this.isPlaying = false;
        this.stopTimeUpdates();
        chrome.runtime.sendMessage({
          type: 'TRACK_STOPPED'
        });
      }

    } catch (error) {
      console.error('Error detecting YouTube Music track:', error);
    }
  }

  attachVideoListeners() {
    if (this.videoListenersAttached) return;
    const video = document.querySelector('video');
    if (!video) return;
    this.videoListenersAttached = true;

    const ensureTrack = () => {
      // make sure we have currentTrack populated
      if (!this.currentTrack) {
        this.detectCurrentTrack();
      }
    };

    video.addEventListener('play', () => {
      this.isPlaying = true;
      ensureTrack();
      if (this.currentTrack) {
        chrome.runtime.sendMessage({ type: 'TRACK_DETECTED', data: this.currentTrack });
      }
      this.startTimeUpdates();
    });

    video.addEventListener('pause', () => {
      this.isPlaying = false;
      ensureTrack();
      if (this.currentTrack) {
        chrome.runtime.sendMessage({ type: 'TRACK_PAUSED', data: this.currentTrack });
      }
      this.stopTimeUpdates();
    });

    video.addEventListener('ended', () => {
      this.isPlaying = false;
      this.stopTimeUpdates();
      chrome.runtime.sendMessage({ type: 'TRACK_STOPPED' });
    });
  }

  getCurrentTime() {
    // Try multiple methods to get current time
    const timeSelectors = [
      '.time-info .ytmusic-player-bar .time-info .current-time',
      '.ytmusic-player-bar .time-info .current-time',
      '#movie_player .ytp-time-current',
      '.ytp-time-current',
      '.time-info .time-info-text',
      '.ytmusic-player-bar .progress-bar .time-info',
    ];

    for (const selector of timeSelectors) {
      const timeElement = document.querySelector(selector);
      if (timeElement) {
        const timeText = timeElement.textContent || timeElement.innerText;
        if (timeText && timeText.includes(':')) {
          return this.parseTimeString(timeText);
        }
      }
    }

    // Try to get time from video element directly
    const videoElement = document.querySelector('video');
    if (videoElement && !isNaN(videoElement.currentTime)) {
      return videoElement.currentTime;
    }

    return null;
  }

  getDuration() {
    // Try multiple methods to get duration
    const durationSelectors = [
      '.time-info .ytmusic-player-bar .time-info .duration',
      '.ytmusic-player-bar .time-info .duration',
      '#movie_player .ytp-time-duration',
      '.ytp-time-duration',
      '.time-info .duration-text',
    ];

    for (const selector of durationSelectors) {
      const durationElement = document.querySelector(selector);
      if (durationElement) {
        const timeText = durationElement.textContent || durationElement.innerText;
        if (timeText && timeText.includes(':')) {
          return this.parseTimeString(timeText);
        }
      }
    }

    // Try to get duration from video element directly
    const videoElement = document.querySelector('video');
    if (videoElement && !isNaN(videoElement.duration)) {
      return videoElement.duration;
    }

    return null;
  }

  parseTimeString(timeStr) {
    const parts = timeStr.trim().split(':');
    if (parts.length === 2) {
      // mm:ss format
      const minutes = parseInt(parts[0], 10) || 0;
      const seconds = parseInt(parts[1], 10) || 0;
      return minutes * 60 + seconds;
    } else if (parts.length === 3) {
      // hh:mm:ss format
      const hours = parseInt(parts[0], 10) || 0;
      const minutes = parseInt(parts[1], 10) || 0;
      const seconds = parseInt(parts[2], 10) || 0;
      return hours * 3600 + minutes * 60 + seconds;
    }
    return null;
  }

  startTimeUpdates() {
    // Clear existing interval
    if (this.timeUpdateInterval) {
      clearInterval(this.timeUpdateInterval);
    }

    // Send time updates every second when playing
    this.timeUpdateInterval = setInterval(() => {
      if (this.isPlaying && this.currentTrack) {
        const currentTime = this.getCurrentTime();
        const duration = this.getDuration();

        if (currentTime !== null) {
          const trackWithTime = {
            ...this.currentTrack,
            currentTime: currentTime,
            duration: duration,
            isPlaying: this.isPlaying
          };

          chrome.runtime.sendMessage({
            type: 'TRACK_PROGRESS',
            data: trackWithTime
          });

          // console.log('Time update:', currentTime, '/', duration);
        }
      }
    }, 200); // Update every 200ms for smooth progress
  }

  stopTimeUpdates() {
    if (this.timeUpdateInterval) {
      clearInterval(this.timeUpdateInterval);
      this.timeUpdateInterval = null;
    }
  }

  destroy() {
    if (this.observer) {
      this.observer.disconnect();
    }
    this.stopTimeUpdates();
  }
}

// Listen for playback commands from desktop app
chrome.runtime.onMessage.addListener(async (message, sender, sendResponse) => {
  console.log('🟡 [Content] Received message:', message);

  if (message.type === 'PLAYBACK_COMMAND') {
    console.log('🟢 [Content] Processing PLAYBACK_COMMAND:', message);
    const universalSelectors = [
      'ytmusic-player-bar button[aria-label*="lay" i]',
      '[role="button"][aria-label*="lay" i]',
      'button[title*="lay" i]'
    ];

    switch (message.command) {
      case 'play':
        // Prefer media API (language-agnostic)
        try {
          const video = document.querySelector('video');
          if (video) {
            await video.play();
            console.log('✅ Played via video.play()');
            sendResponse({ success: true });
            return true;
          }
        } catch (e) { console.log('video.play() failed, falling back:', e); }
        // Try to find and click play button
        const playSelectors = [
          'ytmusic-player-bar #play-pause-button',
          '#play-pause-button',
          '.play-pause-button',
          'ytmusic-player-bar .play-pause-button',
          '[aria-label*="play" i][role="button"]',
          '[title*="play" i]'
        ];

        for (const selector of playSelectors) {
          const playButton = document.querySelector(selector);
          if (playButton) {
            const ariaLabel = playButton.getAttribute('aria-label') || '';
            const title = playButton.getAttribute('title') || '';
            // Check if this is actually a play button (not pause)
            if (ariaLabel.toLowerCase().includes('play') || title.toLowerCase().includes('play')) {
              playButton.click();
              console.log('✅ Clicked play button via', selector);
              sendResponse({ success: true });
              return;
            }
          }
        }

        // Fallback: try to use keyboard event
        try {
          const playEvent = new KeyboardEvent('keydown', { code: 'Space' });
          document.dispatchEvent(playEvent);
          console.log('✅ Triggered play via keyboard event');
          sendResponse({ success: true });
          return;
        } catch (e) {
          console.log('Failed to use keyboard event:', e);
        }

        // Last resort: click any play/pause button we can find

        for (const selector of universalSelectors) {
          const button = document.querySelector(selector);
          if (button) {
            button.click();
            console.log('✅ Clicked universal play button via', selector);
            sendResponse({ success: true });
            return;
          }
        }
        break;

      case 'pause':
        // Prefer media API
        try {
          const video = document.querySelector('video');
          if (video) {
            video.pause();
            console.log('✅ Paused via video.pause()');
            sendResponse({ success: true });
            return true;
          }
        } catch (e) { console.log('video.pause() failed, falling back:', e); }
        // Try to find and click pause button
        const pauseSelectors = [
          'ytmusic-player-bar #play-pause-button',
          '#play-pause-button',
          '.play-pause-button',
          'ytmusic-player-bar .play-pause-button',
          '[aria-label*="pause" i][role="button"]',
          '[title*="pause" i]'
        ];

        for (const selector of pauseSelectors) {
          const pauseButton = document.querySelector(selector);
          if (pauseButton) {
            const ariaLabel = pauseButton.getAttribute('aria-label') || '';
            const title = pauseButton.getAttribute('title') || '';
            // Check if this is actually a pause button (not play)
            if (ariaLabel.toLowerCase().includes('pause') || title.toLowerCase().includes('pause')) {
              pauseButton.click();
              console.log('✅ Clicked pause button via', selector);
              sendResponse({ success: true });
              return;
            }
          }
        }

        // Fallback: try to use keyboard event
        try {
          const pauseEvent = new KeyboardEvent('keydown', { code: 'Space' });
          document.dispatchEvent(pauseEvent);
          console.log('✅ Triggered pause via keyboard event');
          sendResponse({ success: true });
          return;
        } catch (e) {
          console.log('Failed to use keyboard event:', e);
        }

        for (const selector of universalSelectors) {
          const button = document.querySelector(selector);
          if (button) {
            button.click();
            console.log('✅ Clicked universal pause button via', selector);
            sendResponse({ success: true });
            return;
          }
        }
        break;

      case 'next': {
        // Try button
        const nextSelectors = [
          'ytmusic-player-bar #next-button',
          '#next-button',
          'ytmusic-player-bar tp-yt-paper-icon-button[aria-label*="ext" i]'
        ];
        for (const sel of nextSelectors) {
          const btn = document.querySelector(sel);
          if (btn) { btn.click(); console.log('✅ Clicked next via', sel); sendResponse({ success: true }); return true; }
        }
        // Fallback: keyboard
        try {
          document.dispatchEvent(new KeyboardEvent('keydown', { key: 'N', shiftKey: true, bubbles: true }));
          sendResponse({ success: true }); return true;
        } catch (e) {}
        break;
      }

      case 'previous': {
        const prevSelectors = [
          'ytmusic-player-bar #previous-button',
          '#previous-button',
          'ytmusic-player-bar tp-yt-paper-icon-button[aria-label*="revious" i]'
        ];
        for (const sel of prevSelectors) {
          const btn = document.querySelector(sel);
          if (btn) { btn.click(); console.log('✅ Clicked previous via', sel); sendResponse({ success: true }); return true; }
        }
        try {
          document.dispatchEvent(new KeyboardEvent('keydown', { key: 'P', shiftKey: true, bubbles: true }));
          sendResponse({ success: true }); return true;
        } catch (e) {}
        break;
      }

      case 'seek':
        if (message.seekTime !== undefined) {
          // Try to seek using video element directly first (most reliable)
          const videoElement = document.querySelector('video');
          if (videoElement) {
            try {
              videoElement.currentTime = message.seekTime;
              console.log('✅ Seeked via video element to:', message.seekTime);
              sendResponse({ success: true });
              return;
            } catch (e) {
              console.log('Failed to seek via video element:', e);
            }
          }

          // Fallback to progress bar clicking
          const progressSelectors = [
            '.ytmusic-player-bar .progress-bar',
            'ytmusic-player-bar .progress-bar input',
            '#progress-bar',
            '.progress-bar',
            '.ytmusic-player-bar .slider-bar',
            '.time-info .progress-bar'
          ];

          for (const selector of progressSelectors) {
            const progressBar = document.querySelector(selector);
            if (progressBar) {
              const duration = getCurrentDuration();
              if (duration && duration > 0) {
                const seekPercent = Math.max(0, Math.min(1, message.seekTime / duration));

                if (progressBar.tagName === 'INPUT') {
                  // If it's an input element (slider), set value directly
                  progressBar.value = seekPercent * 100;
                  progressBar.dispatchEvent(new Event('input', { bubbles: true }));
                  progressBar.dispatchEvent(new Event('change', { bubbles: true }));
                } else {
                  // Otherwise, click on progress bar
                  const rect = progressBar.getBoundingClientRect();
                  const clickX = rect.left + (rect.width * seekPercent);

                  const clickEvent = new MouseEvent('click', {
                    clientX: clickX,
                    clientY: rect.top + (rect.height / 2),
                    bubbles: true
                  });
                  progressBar.dispatchEvent(clickEvent);
                }

                console.log('✅ Seeked via', selector, 'to:', message.seekTime);
                sendResponse({ success: true });
                return;
              }
            }
          }
        }
        break;
    }

    console.log('❌ [Content] All playback command attempts failed for:', message.command);
    sendResponse({ success: false, error: `Command '${message.command}' could not be executed` });
  } else {
    console.log('🟡 [Content] Ignoring message type:', message.type);
    sendResponse({ success: false, error: 'Unknown message type' });
  }

  return true; // Keep message channel open
});

// Helper function to get current duration
function getCurrentDuration() {
  const durationSelectors = [
    '.time-info .duration',
    '.ytp-time-duration'
  ];

  for (const selector of durationSelectors) {
    const element = document.querySelector(selector);
    if (element) {
      const timeText = element.textContent;
      if (timeText && timeText.includes(':')) {
        const parts = timeText.split(':');
        if (parts.length === 2) {
          return parseInt(parts[0]) * 60 + parseInt(parts[1]);
        }
      }
    }
  }

  // Try video element
  const video = document.querySelector('video');
  if (video && !isNaN(video.duration)) {
    return video.duration;
  }

  return null;
}

// Initialize detector
if (document.readyState === 'loading') {
  document.addEventListener('DOMContentLoaded', () => {
    new YouTubeMusicDetector();
  });
} else {
  new YouTubeMusicDetector();
}
