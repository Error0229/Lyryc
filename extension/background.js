// Background script for Lyryc extension
let currentTrack = null;
let isConnectedToApp = false;

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
        '*://open.spotify.com/*',
        '*://music.youtube.com/*', 
        '*://music.apple.com/*',
        '*://soundcloud.com/*'
      ]
    });
    
    console.log('🟢 [Background] Found', tabs.length, 'music tabs:', tabs.map(t => t.url));
    
    // Send command to all relevant tabs
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
      } catch (error) {
        console.log(`❌ [Background] Could not send command to tab ${tab.id}:`, error);
      }
    }
  } catch (error) {
    console.error('❌ [Background] Error handling playback command:', error);
  }
}

// Send track info to desktop app
function sendTrackToApp(track, messageType = 'TRACK_DETECTED') {
  const message = {
    message_type: messageType,
    data: {
      title: track.title,
      artist: track.artist,
      thumbnail: track.thumbnail,
      source: track.source,
      url: track.url,
      timestamp: track.timestamp,
      is_playing: messageType === 'TRACK_DETECTED',
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

// Listen for messages from content scripts
chrome.runtime.onMessage.addListener((message, sender, sendResponse) => {
  if (message.type === 'TRACK_DETECTED') {
    currentTrack = message.data;
    console.log('Track detected:', currentTrack);
    
    // Send to desktop app
    sendTrackToApp(currentTrack, 'TRACK_DETECTED');
    
    // Update badge
    chrome.action.setBadgeText({
      text: '♪',
      tabId: sender.tab.id
    });
    
    chrome.action.setBadgeBackgroundColor({
      color: '#4CAF50'
    });
    
    sendResponse({ success: true });
  } else if (message.type === 'TRACK_PAUSED') {
    if (currentTrack) {
      sendTrackToApp(currentTrack, 'TRACK_PAUSED');
    }
    
    chrome.action.setBadgeText({
      text: '⏸',
      tabId: sender.tab.id
    });
    
    chrome.action.setBadgeBackgroundColor({
      color: '#FF9800'
    });
    
    sendResponse({ success: true });
  } else if (message.type === 'TRACK_STOPPED') {
    if (currentTrack) {
      sendTrackToApp(currentTrack, 'TRACK_STOPPED');
    }
    
    chrome.action.setBadgeText({
      text: '',
      tabId: sender.tab.id
    });
    
    currentTrack = null;
    
    sendResponse({ success: true });
  } else if (message.type === 'TRACK_PROGRESS') {
    if (currentTrack) {
      // Update current track with progress data
      currentTrack = { ...currentTrack, ...message.data };
      sendTrackToApp(currentTrack, 'TRACK_PROGRESS');
    }
    
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
});

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