// Service-worker side: must run from a user gesture (action click).
export async function startTabCapture(tab) {
  const streamId = await chrome.tabCapture.getMediaStreamId({ targetTabId: tab.id });
  if (!(await chrome.offscreen.hasDocument())) {
    await chrome.offscreen.createDocument({
      url: 'prototype-tabcapture/offscreen.html',
      reasons: ['USER_MEDIA'],
      justification: 'Capture music tab audio for local lyric alignment',
    });
  }
  chrome.runtime.sendMessage({ target: 'offscreen', type: 'start-capture', streamId, tabId: tab.id });
}
