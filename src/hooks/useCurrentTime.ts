import { useState, useEffect } from 'react';

interface UseCurrentTimeOptions {
  isPlaying: boolean;
  startTime?: number;
}

export const useCurrentTime = ({ isPlaying, startTime = 0 }: UseCurrentTimeOptions) => {
  const [currentTime, setCurrentTime] = useState(startTime);

  useEffect(() => {
    if (!isPlaying) return;

    const startedAt = Date.now() - (currentTime * 1000);
    
    const updateTime = () => {
      const elapsed = (Date.now() - startedAt) / 1000;
      setCurrentTime(elapsed);
    };

    const interval = setInterval(updateTime, 100); // Update every 100ms for smooth animation

    return () => {
      clearInterval(interval);
      updateTime(); // Final update
    };
  }, [isPlaying, startTime]);

  const seekTo = (time: number) => {
    setCurrentTime(time);
  };

  return { currentTime, seekTo };
};