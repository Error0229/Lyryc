import React from 'react';
import { motion } from 'framer-motion';
import { invoke } from '@tauri-apps/api/core';
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

  const handlePlayPause = async () => {
    const command = isPlaying ? 'pause' : 'play';
    try {
      // Check WebSocket clients first
      const clientsCount = await invoke('get_websocket_clients_count');
      console.log('🔍 WebSocket clients connected:', clientsCount);
      
      if (clientsCount === 0) {
        console.warn('⚠️ No WebSocket clients connected - using fallback');
        
        // Debug WebSocket server
        try {
          const debugInfo = await invoke('debug_websocket_server');
          console.log('🔍 WebSocket Debug Info:', debugInfo);
        } catch (e) {
          console.error('Failed to get debug info:', e);
        }
        
        onPlayPause();
        return;
      }
      
      const result = await invoke('send_playback_command', { 
        command: command
      });
      console.log('🎵 Successfully sent playback command:', command, result);
      // Optimistically update UI; WebSocket events will reconcile if needed
      onPlayPause();
    } catch (error) {
      console.error('❌ Failed to send playback command:', command, error);
      // Fallback to local control if command fails
      console.log('⚠️ Using fallback local control');
      onPlayPause();
    }
  };

  const handleSeek = async (time: number) => {
    try {
      const result = await invoke('send_playback_command', { 
        command: 'seek',
        seekTime: time 
      });
      console.log('🎵 Successfully sent seek command:', time, result);
      // Don't call onSeek() immediately - let the browser update come through
    } catch (error) {
      console.error('❌ Failed to send seek command:', time, error);
      // Fallback to local control if command fails
      console.log('⚠️ Using fallback local seek');
      onSeek(time);
    }
  };

  const [isDragging, setIsDragging] = React.useState(false);
  const [dragTime, setDragTime] = React.useState<number | null>(null);
  const barRef = React.useRef<HTMLDivElement>(null);

  const effectiveTime = isDragging && dragTime !== null ? dragTime : currentTime;
  const progress = Math.min(effectiveTime / duration, 1);

  const onDragStart = (clientX: number, rect: DOMRect) => {
    const x = Math.max(0, Math.min(clientX - rect.left, rect.width));
    const nt = (x / rect.width) * duration;
    setIsDragging(true);
    setDragTime(nt);
  };

  const onDragMove = (clientX: number, rect: DOMRect) => {
    if (!isDragging) return;
    const x = Math.max(0, Math.min(clientX - rect.left, rect.width));
    const nt = (x / rect.width) * duration;
    setDragTime(nt);
  };

  const onDragEnd = async () => {
    if (isDragging && dragTime !== null) {
      await handleSeek(dragTime);
    }
    setIsDragging(false);
    setDragTime(null);
  };

  React.useEffect(() => {
    const handleWindowMove = (e: MouseEvent) => {
      if (!isDragging || !barRef.current) return;
      const rect = barRef.current.getBoundingClientRect();
      onDragMove(e.clientX, rect);
    };
    const handleWindowUp = () => {
      if (!isDragging) return;
      onDragEnd();
      window.removeEventListener('mousemove', handleWindowMove);
      window.removeEventListener('mouseup', handleWindowUp);
    };

    if (isDragging) {
      window.addEventListener('mousemove', handleWindowMove);
      window.addEventListener('mouseup', handleWindowUp);
    }
    return () => {
      window.removeEventListener('mousemove', handleWindowMove);
      window.removeEventListener('mouseup', handleWindowUp);
    };
  }, [isDragging, dragTime]);

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
          ref={barRef}
          className="w-full h-2 bg-white/20 rounded-full cursor-pointer group select-none"
          onMouseDown={(e) => {
            const rect = e.currentTarget.getBoundingClientRect();
            onDragStart(e.clientX, rect);
          }}
          onTouchStart={(e) => {
            const touch = e.touches[0];
            const rect = e.currentTarget.getBoundingClientRect();
            onDragStart(touch.clientX, rect);
          }}
          onTouchMove={(e) => {
            const touch = e.touches[0];
            const rect = e.currentTarget.getBoundingClientRect();
            onDragMove(touch.clientX, rect);
          }}
          onTouchEnd={onDragEnd}
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
        {/* Previous */}
        <button 
          className="p-2 text-white/60 hover:text-white transition-colors"
          onClick={async () => {
            try {
              await invoke('send_playback_command', { command: 'previous' });
            } catch (e) {
              console.error('Failed to send previous command', e);
            }
          }}
        >
          <svg className="w-6 h-6" fill="currentColor" viewBox="0 0 24 24">
            <path d="M6 6h2v12H6zm3.5 6l8.5 6V6z"/>
          </svg>
        </button>

        {/* Play/Pause */}
        <motion.button
          whileHover={{ scale: 1.05 }}
          whileTap={{ scale: 0.95 }}
          onClick={handlePlayPause}
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

        {/* Next */}
        <button 
          className="p-2 text-white/60 hover:text-white transition-colors"
          onClick={async () => {
            try {
              await invoke('send_playback_command', { command: 'next' });
            } catch (e) {
              console.error('Failed to send next command', e);
            }
          }}
        >
          <svg className="w-6 h-6" fill="currentColor" viewBox="0 0 24 24">
            <path d="M6 18l8.5-6L6 6v12zM16 6v12h2V6h-2z"/>
          </svg>
        </button>
      </div>

      {/* Control note */}
      <div className="text-center mt-4 text-white/40 text-xs">
        🎵 Controls sync with your browser's music player
      </div>
    </div>
  );
};

export default MediaControls;

// window-level handlers to support mouse dragging
function mouseMoveHandler(e: MouseEvent) {
  const el = document.querySelector('.w-full.h-2.bg-white\\/20.rounded-full.cursor-pointer.group.select-none') as HTMLElement | null;
  if (!el) return;
  const rect = el.getBoundingClientRect();
  // No direct access to component state here; this handler is a placeholder to satisfy TS when bundling.
}

function mouseUpHandler() {
  window.removeEventListener('mousemove', mouseMoveHandler);
}
