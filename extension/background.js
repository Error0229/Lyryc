// Background script for Lyryc extension
let currentTrack = null;
let isConnectedToApp = false;
// Last YouTube Music queue snapshot (UPCOMING_TRACKS), resent on reconnect.
let upcoming = null;

// WebSocket connection to desktop app
let websocket = null;
let reconnectAttempts = 0;
let pingInterval = null;
const maxReconnectAttempts = 5;
const reconnectDelay = 3000; // 3 seconds

// Initialize WebSocket connection
function connectToDesktopApp() {
  // Don't create multiple connections
  if (websocket && (websocket.readyState === WebSocket.CONNECTING || websocket.readyState === WebSocket.OPEN)) {
    console.log('🟡 [Background] WebSocket already connecting/connected, skipping');
    return;
  }

  try {
    console.log('🟡 [Background] Attempting WebSocket connection to ws://localhost:8765');
    websocket = new WebSocket('ws://localhost:8765');
    
    websocket.onopen = () => {
      console.log('🟢 [Background] ✅ Connected to desktop app via WebSocket');
      isConnectedToApp = true;
      reconnectAttempts = 0;

      // The app may have started mid-song: tell it what is playing right now.
      if (currentTrack) {
        sendTrackToApp(currentTrack, currentTrack.isPlaying === false ? 'TRACK_PAUSED' : 'TRACK_DETECTED');
      }
      if (upcoming) sendUpcomingToApp(upcoming);
      
      // Send ping to keep connection alive (only create one interval)
      if (!pingInterval) {
        pingInterval = setInterval(() => {
          if (websocket && websocket.readyState === WebSocket.OPEN) {
            console.log('🟢 [Background] Sending ping to desktop app');
            websocket.send(JSON.stringify({
              message_type: 'ping',
              data: {},
              timestamp: Date.now()
            }));
          }
        }, 30000); // Every 30 seconds
      }
    };
    
    websocket.onmessage = (event) => {
      try {
        const message = JSON.parse(event.data);
        console.log('🟢 [Background] Received from desktop app:', message);
        
        // Handle different message types
        switch (message.message_type) {
          case 'connected':
            console.log('🟢 [Background] Desktop app acknowledged connection');
            break;
          case 'pong':
            console.log('🟢 [Background] Received pong from desktop app');
            break;
          case 'PLAYBACK_COMMAND':
            console.log('🟢 [Background] Received PLAYBACK_COMMAND:', message.data);
            // Forward playback command to content script
            handlePlaybackCommand(message.data);
            break;
          default:
            console.log('🟡 [Background] Unknown message type:', message.message_type);
        }
      } catch (error) {
        console.error('❌ [Background] Failed to parse message from desktop app:', error);
      }
    };
    
    websocket.onclose = (event) => {
      console.log('🔴 [Background] ❌ Disconnected from desktop app', event.code, event.reason);
      isConnectedToApp = false;
      
      // Clear ping interval
      if (pingInterval) {
        clearInterval(pingInterval);
        pingInterval = null;
      }
      
      websocket = null;
      
      // Attempt to reconnect
      if (reconnectAttempts < maxReconnectAttempts) {
        reconnectAttempts++;
        console.log(`🟡 [Background] Attempting to reconnect (${reconnectAttempts}/${maxReconnectAttempts})...`);
        setTimeout(connectToDesktopApp, reconnectDelay * reconnectAttempts);
      } else {
        console.log('🔴 [Background] Max reconnect attempts reached, giving up');
      }
    };
    
    websocket.onerror = (error) => {
      console.error('🔴 [Background] ❌ WebSocket error:', error);
      isConnectedToApp = false;
    };
    
  } catch (error) {
    console.error('🔴 [Background] ❌ Failed to create WebSocket connection:', error);
    isConnectedToApp = false;
  }
}

// Handle playback commands from desktop app
async function handlePlaybackCommand(commandData) {
  console.log('🟢 [Background] Handling playback command:', commandData);
  
  try {
    // Get all tabs with music sites
    const tabs = await chrome.tabs.query({
      url: [
        'https://open.spotify.com/*',
        'https://music.youtube.com/*', 
        'https://music.apple.com/*',
        'https://soundcloud.com/*'
      ]
    });
    
    console.log('🟢 [Background] Found', tabs.length, 'music tabs:', tabs.map(t => t.url));
    
    // Send command to all relevant tabs
    let anySucceeded = false;
    for (const tab of tabs) {
      try {
        console.log('🟢 [Background] Sending PLAYBACK_COMMAND to tab', tab.id, ':', {
          type: 'PLAYBACK_COMMAND',
          command: commandData.command,
          seekTime: commandData.seekTime
        });
        
        const response = await chrome.tabs.sendMessage(tab.id, {
          type: 'PLAYBACK_COMMAND',
          command: commandData.command,
          seekTime: commandData.seekTime
        });
        
        console.log('🟢 [Background] Response from tab', tab.id, ':', response);
        if (response && response.success) {
          anySucceeded = true;
        }
      } catch (error) {
        console.log(`❌ [Background] Could not send command to tab ${tab.id}:`, error);
      }
    }

    // Optimistically echo play/pause state to desktop app if a tab handled it
    if (anySucceeded && currentTrack && (commandData.command === 'play' || commandData.command === 'pause')) {
      const playing = commandData.command === 'play';
      currentTrack = { ...currentTrack, isPlaying: playing };
      sendTrackToApp(currentTrack, playing ? 'TRACK_DETECTED' : 'TRACK_PAUSED');
    }
  } catch (error) {
    console.error('❌ [Background] Error handling playback command:', error);
  }
}

// Send track info to desktop app
function sendTrackToApp(track, messageType = 'TRACK_DETECTED') {
  const inferredIsPlaying =
    typeof track.isPlaying === 'boolean'
      ? track.isPlaying
      : !(messageType === 'TRACK_PAUSED' || messageType === 'TRACK_STOPPED');

  const message = {
    message_type: messageType,
    data: {
      title: track.title,
      artist: track.artist,
      thumbnail: track.thumbnail,
      source: track.source,
      url: track.url,
      timestamp: track.timestamp,
      is_playing: inferredIsPlaying,
      currentTime: track.currentTime || 0,
      duration: track.duration || 0
    },
    timestamp: Date.now()
  };

  if (websocket && websocket.readyState === WebSocket.OPEN) {
    websocket.send(JSON.stringify(message));
  } else {
    // Store in extension storage as fallback
    chrome.storage.local.set({ currentTrack: track });
    console.log('WebSocket not connected, stored track locally');
  }
}

// The YouTube Music queue: what plays now and next. The app ignores it unless
// its pre-align setting is on.
function sendUpcomingToApp(state) {
  if (websocket && websocket.readyState === WebSocket.OPEN) {
    websocket.send(JSON.stringify({ message_type: 'UPCOMING_TRACKS', data: state, timestamp: Date.now() }));
  }
}

// The track fields a content script may report, and only those that are set:
// a partial message never blanks what's known. null is kept: a new song's
// unknown length must not inherit the last song's.
const TRACK_FIELDS = ['title', 'artist', 'thumbnail', 'source', 'url', 'timestamp', 'currentTime', 'duration', 'isPlaying'];
function trackFields(data) {
  const out = {};
  for (const key of TRACK_FIELDS) {
    if (data && data[key] !== undefined) out[key] = data[key];
  }
  return out;
}

// Listen for messages from content scripts
chrome.runtime.onMessage.addListener((message, sender, sendResponse) => {
  if (message.type === 'TRACK_DETECTED') {
    currentTrack = { ...trackFields(message.data), isPlaying: true };
    console.log('Track detected:', currentTrack);
    
    // Send to desktop app
    sendTrackToApp(currentTrack, 'TRACK_DETECTED');
    
    // Update badge
    chrome.action.setBadgeText({
      text: '♪',
      tabId: sender.tab.id
    });
    
    chrome.action.setBadgeBackgroundColor({
      color: '#3F6F75' // brand teal: playing
    });
    
    sendResponse({ success: true });
  } else if (message.type === 'TRACK_PAUSED') {
    // The message names the track: a new song still loading arrives paused,
    // and is announced now rather than once it plays.
    currentTrack = { ...currentTrack, ...trackFields(message.data), isPlaying: false };
    if (currentTrack.title) sendTrackToApp(currentTrack, 'TRACK_PAUSED');
    
    chrome.action.setBadgeText({
      text: '⏸',
      tabId: sender.tab.id
    });
    
    chrome.action.setBadgeBackgroundColor({
      color: '#6B7280' // neutral: paused
    });
    
    sendResponse({ success: true });
  } else if (message.type === 'TRACK_STOPPED') {
    if (currentTrack) {
      const trackToSend = { ...currentTrack, isPlaying: false };
      sendTrackToApp(trackToSend, 'TRACK_STOPPED');
    }
    
    chrome.action.setBadgeText({
      text: '',
      tabId: sender.tab.id
    });
    
    currentTrack = null;
    
    sendResponse({ success: true });
  } else if (message.type === 'UPCOMING_TRACKS') {
    upcoming = message.data;
    sendUpcomingToApp(upcoming);
    sendResponse({ success: true });
  } else if (message.type === 'TRACK_PROGRESS') {
    // Progress carries the full track. Adopt it even if we have none: Chrome
    // restarts idle service workers, which wipes currentTrack mid-song.
    currentTrack = { ...currentTrack, ...trackFields(message.data) };
    sendTrackToApp(currentTrack, 'TRACK_PROGRESS');
    
    sendResponse({ success: true });
  }
  
  return true; // Keep message channel open for async response
});

// Handle tab updates
chrome.tabs.onUpdated.addListener((tabId, changeInfo, tab) => {
  if (changeInfo.status === 'complete') {
    const url = tab.url;
    if (url && (url.includes('open.spotify.com') || 
                url.includes('music.youtube.com') || 
                url.includes('music.apple.com') || 
                url.includes('soundcloud.com'))) {
      // Reset badge when navigating to music sites
      chrome.action.setBadgeText({
        text: '',
        tabId: tabId
      });
    }
  }
});

// Initialize connection on startup
chrome.runtime.onStartup.addListener(() => {
  console.log('🟢 [Background] Runtime startup - connecting to desktop app');
  connectToDesktopApp();
});

chrome.runtime.onInstalled.addListener(() => {
  console.log('🟢 [Background] Extension installed - connecting to desktop app');
  connectToDesktopApp();
  injectIntoOpenTabs();
});

// Manifest content scripts only reach pages loaded after install/reload. Inject
// them into music tabs that were already open, so no page refresh is needed.
async function injectIntoOpenTabs() {
  for (const { matches, js, world } of chrome.runtime.getManifest().content_scripts) {
    const tabs = await chrome.tabs.query({ url: matches });
    for (const tab of tabs) {
      chrome.scripting
        .executeScript({ target: { tabId: tab.id }, files: js, world: world || 'ISOLATED' })
        .catch((e) => console.log('🟡 [Background] Could not inject into tab', tab.id, e.message));
    }
  }
}

// Also try to connect when service worker starts
console.log('🟢 [Background] Service worker started - connecting to desktop app');
connectToDesktopApp();

// Periodically check connection and reconnect if needed
setInterval(() => {
  if (!isConnectedToApp || !websocket || websocket.readyState !== WebSocket.OPEN) {
    console.log('🟡 [Background] Connection check - not connected, attempting reconnect');
    connectToDesktopApp();
  } else {
    console.log('🟢 [Background] Connection check - WebSocket is healthy');
  }
}, 10000); // Check every 10 seconds

// Export current track for popup
chrome.runtime.onMessage.addListener((message, sender, sendResponse) => {
  if (message.type === 'GET_CURRENT_TRACK') {
    sendResponse({ track: currentTrack, connected: isConnectedToApp });
  }
});
