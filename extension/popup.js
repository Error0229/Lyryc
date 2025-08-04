// Popup script for Lyryc extension
document.addEventListener('DOMContentLoaded', async () => {
  const statusDot = document.getElementById('statusDot');
  const statusText = document.getElementById('statusText');
  const trackInfo = document.getElementById('trackInfo');
  const noTrack = document.getElementById('noTrack');
  const trackTitle = document.getElementById('trackTitle');
  const trackArtist = document.getElementById('trackArtist');
  const trackSource = document.getElementById('trackSource');
  const openAppBtn = document.getElementById('openAppBtn');

  // Get current track from background script
  try {
    const response = await chrome.runtime.sendMessage({ type: 'GET_CURRENT_TRACK' });
    
    if (response.connected) {
      statusDot.classList.add('connected');
      statusText.textContent = 'Connected to Lyryc App';
    } else {
      statusDot.classList.add('disconnected');
      statusText.textContent = 'Desktop app not connected';
    }

    if (response.track) {
      // Show track info
      trackInfo.style.display = 'block';
      noTrack.style.display = 'none';
      
      trackTitle.textContent = response.track.title;
      trackArtist.textContent = `by ${response.track.artist}`;
      trackSource.textContent = response.track.source === 'spotify' ? 'Spotify' : 'YouTube Music';
    } else {
      // No track
      trackInfo.style.display = 'none';
      noTrack.style.display = 'block';
    }
  } catch (error) {
    console.error('Failed to get current track:', error);
    statusDot.classList.add('disconnected');
    statusText.textContent = 'Extension error';
    trackInfo.style.display = 'none';
    noTrack.style.display = 'block';
  }

  // Open app button (placeholder for now)
  openAppBtn.addEventListener('click', () => {
    // TODO: Implement native messaging to open desktop app
    chrome.tabs.create({ url: 'https://github.com/your-username/lyryc' });
  });
});