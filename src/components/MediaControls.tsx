import React from 'react';
import { motion } from 'framer-motion';
import { useLyricsStore } from '../stores/lyricsStore';

interface MediaControlsProps {
  currentTime: number;
  duration?: number;
  isPlaying: boolean;
  onPlayPause: () => void;
  onSeek: (time: number) => void;
}

const MediaControls: React.FC<MediaControlsProps> = ({
  currentTime,
  duration = 180, // Default 3 minutes
  isPlaying,
  onPlayPause,
  onSeek
}) => {
  const { currentTrack } = useLyricsStore();

  const formatTime = (time: number) => {
    const minutes = Math.floor(time / 60);
    const seconds = Math.floor(time % 60);
    return `${minutes}:${seconds.toString().padStart(2, '0')}`;
  };

  const progress = Math.min(currentTime / duration, 1);

  return (
    <div className="bg-black/20 backdrop-blur-sm rounded-xl p-6 mt-6">
      {/* Track info */}
      <div className="flex items-center space-x-4 mb-4">
        {currentTrack?.thumbnail && (
          <img 
            src={currentTrack.thumbnail} 
            alt="Album art"
            className="w-12 h-12 rounded-lg object-cover"
          />
        )}
        <div className="flex-1 min-w-0">
          <div className="text-white font-medium truncate">
            {currentTrack?.title || 'Unknown Track'}
          </div>
          <div className="text-white/60 text-sm truncate">
            {currentTrack?.artist || 'Unknown Artist'}
          </div>
        </div>
      </div>

      {/* Progress bar */}
      <div className="mb-4">
        <div 
          className="w-full h-2 bg-white/20 rounded-full cursor-pointer group"
          onClick={(e) => {
            const rect = e.currentTarget.getBoundingClientRect();
            const clickX = e.clientX - rect.left;
            const newTime = (clickX / rect.width) * duration;
            onSeek(newTime);
          }}
        >
          <div 
            className="h-full bg-gradient-to-r from-blue-400 to-purple-400 rounded-full relative transition-all group-hover:shadow-lg"
            style={{ width: `${progress * 100}%` }}
          >
            <div className="absolute right-0 top-1/2 transform translate-x-1/2 -translate-y-1/2 w-4 h-4 bg-white rounded-full shadow-lg opacity-0 group-hover:opacity-100 transition-opacity" />
          </div>
        </div>
        
        {/* Time labels */}
        <div className="flex justify-between text-sm text-white/60 mt-2">
          <span>{formatTime(currentTime)}</span>
          <span>{formatTime(duration)}</span>
        </div>
      </div>

      {/* Controls */}
      <div className="flex items-center justify-center space-x-6">
        {/* Previous (placeholder) */}
        <button className="p-2 text-white/60 hover:text-white transition-colors">
          <svg className="w-6 h-6" fill="currentColor" viewBox="0 0 24 24">
            <path d="M6 6h2v12H6zm3.5 6l8.5 6V6z"/>
          </svg>
        </button>

        {/* Play/Pause */}
        <motion.button
          whileHover={{ scale: 1.05 }}
          whileTap={{ scale: 0.95 }}
          onClick={onPlayPause}
          className="p-4 bg-white/20 hover:bg-white/30 rounded-full transition-colors"
        >
          {isPlaying ? (
            <svg className="w-8 h-8 text-white" fill="currentColor" viewBox="0 0 24 24">
              <path d="M6 19h4V5H6v14zm8-14v14h4V5h-4z"/>
            </svg>
          ) : (
            <svg className="w-8 h-8 text-white" fill="currentColor" viewBox="0 0 24 24">
              <path d="M8 5v14l11-7z"/>
            </svg>
          )}
        </motion.button>

        {/* Next (placeholder) */}
        <button className="p-2 text-white/60 hover:text-white transition-colors">
          <svg className="w-6 h-6" fill="currentColor" viewBox="0 0 24 24">
            <path d="M6 18l8.5-6L6 6v12zM16 6v12h2V6h-2z"/>
          </svg>
        </button>
      </div>

      {/* Demo note */}
      <div className="text-center mt-4 text-white/40 text-xs">
        🎯 Demo Mode - Controls simulate playback
      </div>
    </div>
  );
};

export default MediaControls;