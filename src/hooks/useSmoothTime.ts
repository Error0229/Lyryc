import { useState, useEffect, useRef, useCallback } from 'react';

interface IndependentTimerOptions {
  isPlaying: boolean;
  syncInterval?: number; // How often to sync with browser time (ms)
  updateInterval?: number; // How often to update frontend time (ms)
}

export const useIndependentTimer = (
  browserTime: number,
  options: IndependentTimerOptions
) => {
  const { isPlaying, syncInterval = 5000, updateInterval = 100 } = options;
  const [currentTime, setCurrentTime] = useState(browserTime);
  
  // Internal timer state
  const timerRef = useRef<NodeJS.Timeout>();
  const syncTimerRef = useRef<NodeJS.Timeout>();
  const lastSyncTime = useRef(browserTime);
  const lastSyncTimestamp = useRef(Date.now());
  const playbackRate = useRef(1.0); // Normal playback speed

  // Sync with browser time periodically
  const syncWithBrowser = useCallback(() => {
    const now = Date.now();
    const timeSinceLastSync = (now - lastSyncTimestamp.current) / 1000;
    const expectedTime = lastSyncTime.current + timeSinceLastSync;
    const actualTime = browserTime;
    
    // Calculate drift between our timer and browser time
    const drift = Math.abs(actualTime - expectedTime);
    
    console.log('🔄 Time Sync:', {
      expected: expectedTime.toFixed(2),
      actual: actualTime.toFixed(2),
      drift: drift.toFixed(2),
      correction: drift > 1 ? 'APPLIED' : 'none'
    });
    
    // If drift is more than 0.5 seconds, sync immediately (reduced from 1.0 for faster correction)
    if (drift > 0.5) {
      console.log('⚠️ Large drift detected, syncing to browser time');
      setCurrentTime(actualTime);
      lastSyncTime.current = actualTime;
    } else {
      // Small drift, update reference for smooth correction
      lastSyncTime.current = browserTime;
    }
    
    lastSyncTimestamp.current = now;
  }, [browserTime]);

  // Update browser sync reference when browser time changes
  useEffect(() => {
    syncWithBrowser();
  }, [browserTime, syncWithBrowser]);

  // Independent timer that runs continuously
  useEffect(() => {
    // Clear existing timers
    if (timerRef.current) clearInterval(timerRef.current);
    if (syncTimerRef.current) clearInterval(syncTimerRef.current);

    if (isPlaying) {
      // Main timer - updates frontend time smoothly
      timerRef.current = setInterval(() => {
        setCurrentTime(prevTime => prevTime + (updateInterval / 1000) * playbackRate.current);
      }, updateInterval);

      // Sync timer - periodically syncs with browser
      syncTimerRef.current = setInterval(() => {
        syncWithBrowser();
      }, syncInterval);

      console.log('▶️ Independent timer started');
    } else {
      console.log('⏸️ Independent timer paused');
    }

    return () => {
      if (timerRef.current) clearInterval(timerRef.current);
      if (syncTimerRef.current) clearInterval(syncTimerRef.current);
    };
  }, [isPlaying, updateInterval, syncInterval, syncWithBrowser]);

  // Handle play state changes - sync immediately and reset timer
  useEffect(() => {
    console.log('🎵 Play state changed:', isPlaying, 'Browser time:', browserTime);
    setCurrentTime(browserTime);
    lastSyncTime.current = browserTime;
    lastSyncTimestamp.current = Date.now();
  }, [isPlaying, browserTime]);

  return {
    currentTime,
    syncWithBrowser,
    setPlaybackRate: (rate: number) => {
      playbackRate.current = rate;
    }
  };
};

// Simpler version for basic use cases
export const useSimpleTimer = (
  browserTime: number,
  isPlaying: boolean,
  updateInterval: number = 100
) => {
  const [currentTime, setCurrentTime] = useState(browserTime);
  const intervalRef = useRef<NodeJS.Timeout>();
  const lastUpdateRef = useRef(Date.now());

  // Sync when browser time changes
  useEffect(() => {
    setCurrentTime(browserTime);
    lastUpdateRef.current = Date.now();
  }, [browserTime]);

  // Independent timer
  useEffect(() => {
    if (intervalRef.current) {
      clearInterval(intervalRef.current);
    }

    if (isPlaying) {
      intervalRef.current = setInterval(() => {
        const now = Date.now();
        const elapsed = (now - lastUpdateRef.current) / 1000;
        setCurrentTime(prevTime => prevTime + elapsed);
        lastUpdateRef.current = now;
      }, updateInterval);
    }

    return () => {
      if (intervalRef.current) {
        clearInterval(intervalRef.current);
      }
    };
  }, [isPlaying, updateInterval]);

  return currentTime;
};