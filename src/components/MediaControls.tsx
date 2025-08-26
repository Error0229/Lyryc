import React, { useState } from 'react';
import { motion, AnimatePresence } from 'framer-motion';
import { invoke } from '@tauri-apps/api/core';
import { useLyricsStore } from '../stores/lyricsStore';
import { useOffsetStore } from '../stores/offsetStore';
import { useViewModeStore } from '../stores/viewModeStore';
import AlignmentTester from './AlignmentTester';

interface MediaControlsProps {
  currentTime: number;
  duration?: number;
  isPlaying: boolean;
  onPlayPause: () => void;
  onSeek: (time: number) => void;
  onOpenStyleControls?: () => void;
}

const MediaControls: React.FC<MediaControlsProps> = ({
  currentTime,
  duration = 180,
  isPlaying,
  onPlayPause,
  onSeek,
  onOpenStyleControls
}) => {
  const { currentTrack } = useLyricsStore();
  const { viewMode, setViewMode } = useViewModeStore();
  const { 
    getTrackOffset, 
    setTrackOffset, 
    globalOffset, 
    setGlobalOffset,
    getTotalOffset,
    clearTrackOffset
  } = useOffsetStore();

  const [showAdvanced, setShowAdvanced] = useState(false);
  const [isDragging, setIsDragging] = useState(false);
  const [dragTime, setDragTime] = useState<number | null>(null);
  const barRef = React.useRef<HTMLDivElement>(null);

  const formatTime = (time: number) => {
    const minutes = Math.floor(time / 60);
    const seconds = Math.floor(time % 60);
    return `${minutes}:${seconds.toString().padStart(2, '0')}`;
  };

  const formatOffset = (offsetSeconds: number) => {
    const sign = offsetSeconds >= 0 ? '+' : '';
    return `${sign}${offsetSeconds.toFixed(1)}s`;
  };

  const handlePlayPause = async () => {
    const command = isPlaying ? 'pause' : 'play';
    try {
      const clientsCount = await invoke('get_websocket_clients_count');
      if (clientsCount === 0) {
        onPlayPause();
        return;
      }
      
      await invoke('send_playback_command', { command });
      onPlayPause();
    } catch (error) {
      console.error('Failed to send playback command:', error);
      onPlayPause();
    }
  };

  const handleSeek = async (time: number) => {
    try {
      await invoke('send_playback_command', { 
        command: 'seek',
        seekTime: time 
      });
    } catch (error) {
      console.error('Failed to send seek command:', error);
      onSeek(time);
    }
  };

  const handlePrevNext = async (direction: 'previous' | 'next') => {
    try {
      await invoke('send_playback_command', { command: direction });
    } catch (e) {
      console.error(`Failed to send ${direction} command`, e);
    }
  };

  // Progress bar interaction
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

  // Offset controls
  const trackOffset = currentTrack ? getTrackOffset(currentTrack.artist, currentTrack.title) : 0;
  const totalOffset = currentTrack ? getTotalOffset(currentTrack.artist, currentTrack.title) : 0;

  const adjustOffset = (delta: number, isGlobal = false) => {
    if (!currentTrack) return;
    if (isGlobal) {
      setGlobalOffset(globalOffset + delta);
    } else {
      setTrackOffset(currentTrack.artist, currentTrack.title, trackOffset + delta);
    }
  };

  const resetOffset = (isGlobal = false) => {
    if (!currentTrack) return;
    if (isGlobal) {
      setGlobalOffset(0);
    } else {
      clearTrackOffset(currentTrack.artist, currentTrack.title);
    }
  };

  return (
    <motion.div
      className="relative"
      initial={{ opacity: 0, scale: 0.8 }}
      animate={{ opacity: 1, scale: 1 }}
      exit={{ opacity: 0, scale: 0.8 }}
      transition={{ type: "spring", stiffness: 300, damping: 30 }}
    >
      {/* Main Compact Control Bar - Apple Liquid Glass Style */}
      <div className="
        bg-black/20 backdrop-blur-xl border border-white/10 
        rounded-full px-3 py-1.5 shadow-2xl
        flex items-center space-x-3 min-w-max
      " style={{
        background: 'linear-gradient(135deg, rgba(255,255,255,0.1) 0%, rgba(255,255,255,0.05) 100%)',
        backdropFilter: 'blur(20px) saturate(180%)',
        boxShadow: '0 8px 32px rgba(0,0,0,0.3), inset 0 1px 0 rgba(255,255,255,0.1)'
      }}>
        
        {/* Play/Pause Button */}
        <motion.button
          whileHover={{ scale: 1.1 }}
          whileTap={{ scale: 0.9 }}
          onClick={handlePlayPause}
          className="w-6 h-6 bg-white/10 hover:bg-white/20 rounded-full flex items-center justify-center transition-all duration-200"
        >
          {isPlaying ? (
            <svg className="w-2.5 h-2.5 text-white" fill="currentColor" viewBox="0 0 24 24">
              <path d="M6 19h4V5H6v14zm8-14v14h4V5h-4z"/>
            </svg>
          ) : (
            <svg className="w-2.5 h-2.5 text-white ml-0.5" fill="currentColor" viewBox="0 0 24 24">
              <path d="M8 5v14l11-7z"/>
            </svg>
          )}
        </motion.button>

        {/* Compact Progress Bar */}
        <div className="flex items-center space-x-2 flex-1 max-w-48">
          <span className="text-xs text-white/60 font-mono w-8 text-right">{formatTime(effectiveTime)}</span>
          
          <div 
            ref={barRef}
            className="flex-1 h-1 bg-white/10 rounded-full cursor-pointer group relative"
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
              className="h-full bg-gradient-to-r from-white/60 to-white/40 rounded-full transition-all relative"
              style={{ width: `${progress * 100}%` }}
            >
              <div className="absolute right-0 top-1/2 transform translate-x-1/2 -translate-y-1/2 w-2 h-2 bg-white rounded-full shadow-sm opacity-0 group-hover:opacity-100 transition-opacity" />
            </div>
          </div>

          <span className="text-xs text-white/60 font-mono w-8">{formatTime(duration)}</span>
        </div>

        {/* Quick Actions */}
        <div className="flex items-center space-x-1">
          {/* Sync Quick Controls */}
          <div className="flex items-center bg-white/5 rounded-full px-2 py-0.5">
            <button
              onClick={() => adjustOffset(-0.1)}
              className="w-4 h-4 text-white/60 hover:text-white rounded-full hover:bg-white/10 transition-all text-xs flex items-center justify-center"
            >
              −
            </button>
            <span className="text-xs font-mono text-white/70 px-1 min-w-[24px] text-center">
              {formatOffset(totalOffset)}
            </span>
            <button
              onClick={() => adjustOffset(0.1)}
              className="w-4 h-4 text-white/60 hover:text-white rounded-full hover:bg-white/10 transition-all text-xs flex items-center justify-center"
            >
              +
            </button>
          </div>

          {/* More Options Toggle */}
          <motion.button
            whileHover={{ scale: 1.1 }}
            whileTap={{ scale: 0.9 }}
            onClick={() => setShowAdvanced(!showAdvanced)}
            className={`w-5 h-5 rounded-full transition-all duration-200 flex items-center justify-center ${
              showAdvanced 
                ? 'bg-white/20 text-white' 
                : 'text-white/50 hover:text-white/80 hover:bg-white/10'
            }`}
          >
            <svg className="w-2.5 h-2.5" fill="currentColor" viewBox="0 0 24 24">
              <path d="M12 8c1.1 0 2-.9 2-2s-.9-2-2-2-2 .9-2 2 .9 2 2 2zm0 2c-1.1 0-2 .9-2 2s.9 2 2 2 2-.9 2-2-.9-2-2-2zm0 6c-1.1 0-2 .9-2 2s.9 2 2 2 2-.9 2-2-.9-2-2-2z"/>
            </svg>
          </motion.button>
        </div>
      </div>

      {/* Advanced Controls Panel */}
      <AnimatePresence>
        {showAdvanced && (
          <motion.div
            initial={{ opacity: 0, y: -10, scale: 0.9 }}
            animate={{ opacity: 1, y: 0, scale: 1 }}
            exit={{ opacity: 0, y: -10, scale: 0.9 }}
            transition={{ type: "spring", stiffness: 400, damping: 30 }}
            className="absolute bottom-full mb-2 left-1/2 transform -translate-x-1/2 w-72"
          >
            <div className="
              bg-black/20 backdrop-blur-xl border border-white/10 
              rounded-2xl p-4 shadow-2xl space-y-3
            " style={{
              background: 'linear-gradient(135deg, rgba(255,255,255,0.15) 0%, rgba(255,255,255,0.05) 100%)',
              backdropFilter: 'blur(20px) saturate(180%)',
              boxShadow: '0 8px 32px rgba(0,0,0,0.4), inset 0 1px 0 rgba(255,255,255,0.2)'
            }}>
              
              {/* Header with View Mode and Style */}
              <div className="flex items-center justify-between">
                <div className="flex items-center space-x-2">
                  <button
                    onClick={() => setViewMode(viewMode === 'highlighted' ? 'plain' : 'highlighted')}
                    className={`
                      px-2 py-1 rounded-lg text-xs font-medium transition-all
                      ${viewMode === 'highlighted' 
                        ? 'bg-blue-500/30 text-blue-200 border border-blue-400/50' 
                        : 'bg-white/10 text-white/70 border border-white/20 hover:bg-white/20'
                      }
                    `}
                  >
                    {viewMode === 'highlighted' ? '✨ Synced' : '📝 Plain'}
                  </button>

                  {onOpenStyleControls && (
                    <button
                      onClick={onOpenStyleControls}
                      className="px-2 py-1 bg-purple-500/20 hover:bg-purple-500/30 text-purple-200 rounded-lg text-xs font-medium transition-all border border-purple-400/30"
                    >
                      🎨 Style
                    </button>
                  )}
                </div>

                <div className="flex items-center space-x-1">
                  <button 
                    className="w-5 h-5 text-white/50 hover:text-white/80 hover:bg-white/10 rounded-full flex items-center justify-center transition-all text-xs"
                    onClick={() => handlePrevNext('previous')}
                  >
                    ⏮
                  </button>
                  <button 
                    className="w-5 h-5 text-white/50 hover:text-white/80 hover:bg-white/10 rounded-full flex items-center justify-center transition-all text-xs"
                    onClick={() => handlePrevNext('next')}
                  >
                    ⏭
                  </button>
                </div>
              </div>

              {/* Detailed Sync Controls */}
              <div className="space-y-2">
                <div className="text-xs text-white/60 font-medium">Track Sync</div>
                <div className="flex items-center justify-center space-x-1">
                  <button onClick={() => adjustOffset(-0.5)} className="px-2 py-1 bg-red-500/20 hover:bg-red-500/30 text-white rounded-md text-xs transition-all">-0.5</button>
                  <button onClick={() => adjustOffset(-0.1)} className="px-2 py-1 bg-red-500/15 hover:bg-red-500/25 text-white rounded-md text-xs transition-all">-0.1</button>
                  <button onClick={() => resetOffset(false)} className="px-2 py-1 bg-white/10 hover:bg-white/20 text-white rounded-md text-xs transition-all">Reset</button>
                  <button onClick={() => adjustOffset(0.1)} className="px-2 py-1 bg-green-500/15 hover:bg-green-500/25 text-white rounded-md text-xs transition-all">+0.1</button>
                  <button onClick={() => adjustOffset(0.5)} className="px-2 py-1 bg-green-500/20 hover:bg-green-500/30 text-white rounded-md text-xs transition-all">+0.5</button>
                </div>
                
                <div className="text-xs text-white/60 font-medium">Global Sync</div>
                <div className="flex items-center justify-center space-x-2">
                  <button onClick={() => adjustOffset(-0.2, true)} className="px-2 py-1 bg-orange-500/20 hover:bg-orange-500/30 text-orange-200 rounded-md text-xs transition-all">-0.2</button>
                  <span className="font-mono text-orange-300 text-xs bg-white/5 px-2 py-1 rounded-md">{formatOffset(globalOffset)}</span>
                  <button onClick={() => adjustOffset(0.2, true)} className="px-2 py-1 bg-orange-500/20 hover:bg-orange-500/30 text-orange-200 rounded-md text-xs transition-all">+0.2</button>
                </div>
              </div>

              {/* Alignment Tester */}
              <div className="pt-2 border-t border-white/10">
                <div className="text-xs text-white/60 font-medium mb-2">Alignment Test</div>
                <AlignmentTester totalDurationSec={duration} />
              </div>
            </div>
          </motion.div>
        )}
      </AnimatePresence>
    </motion.div>
  );
};

export default MediaControls;