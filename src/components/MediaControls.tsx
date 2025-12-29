import React, { useState } from 'react';
import { motion, AnimatePresence } from 'framer-motion';
import { invoke } from '@tauri-apps/api/core';
import { useAppState, useConfigStore, useOffsetConfig } from '../stores';
import AlignmentTester from './AlignmentTester';
import TranslationSettings from './TranslationSettings';
import HiraganaToggle from './HiraganaToggle';

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
  const { currentTrack } = useAppState();
  const viewMode = useConfigStore((s) => s.viewMode);
  const setViewMode = useConfigStore((s) => s.setViewMode);
  const {
    getTrackOffset,
    setTrackOffset,
    globalOffset,
    setGlobalOffset,
    getTotalOffset,
  } = useOffsetConfig();

  // Clear track offset helper
  const clearTrackOffset = (artist: string, title: string) => setTrackOffset(artist, title, 0);

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
    if (!currentTrack) {
      console.log('[MediaControls] No current track, offset adjustment skipped');
      return;
    }
    if (isGlobal) {
      const newGlobalOffset = globalOffset + delta;
      setGlobalOffset(newGlobalOffset);
      console.log(`[MediaControls] Global offset adjusted by ${delta}s: ${globalOffset.toFixed(2)}s → ${newGlobalOffset.toFixed(2)}s`);
    } else {
      const newTrackOffset = trackOffset + delta;
      setTrackOffset(currentTrack.artist, currentTrack.title, newTrackOffset);
      console.log(`[MediaControls] Track offset adjusted by ${delta}s: ${trackOffset.toFixed(2)}s → ${newTrackOffset.toFixed(2)}s for "${currentTrack.title}" by "${currentTrack.artist}"`);
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
            className="absolute bottom-full mb-2 left-1/2 transform -translate-x-1/2 w-64"
          >
            <div className="
              bg-black/90 backdrop-blur-xl border border-white/20 
              rounded-xl p-3 shadow-2xl space-y-2
            " style={{
              background: 'linear-gradient(135deg, rgba(0,0,0,0.95) 0%, rgba(0,0,0,0.85) 100%)',
              backdropFilter: 'blur(20px) saturate(180%)',
              boxShadow: '0 8px 32px rgba(0,0,0,0.6), inset 0 1px 0 rgba(255,255,255,0.1)'
            }}>
              
              {/* Header Row */}
              <div className="flex items-center justify-between">
                <div className="flex items-center space-x-1">
                  <button
                    onClick={() => setViewMode(viewMode === 'highlighted' ? 'plain' : 'highlighted')}
                    className={`
                      px-1.5 py-0.5 rounded text-xs transition-all
                      ${viewMode === 'highlighted' 
                        ? 'bg-blue-500/30 text-blue-200' 
                        : 'bg-white/10 text-white/70 hover:bg-white/20'
                      }
                    `}
                  >
                    {viewMode === 'highlighted' ? 'Synced' : 'Plain'}
                  </button>

                  {onOpenStyleControls && (
                    <button
                      onClick={onOpenStyleControls}
                      className="px-1.5 py-0.5 bg-purple-500/20 hover:bg-purple-500/30 text-purple-200 rounded text-xs transition-all"
                    >
                      Style
                    </button>
                  )}
                </div>

                <div className="flex items-center space-x-0.5">
                  <button 
                    className="w-4 h-4 text-white/50 hover:text-white/80 hover:bg-white/10 rounded text-xs flex items-center justify-center transition-all"
                    onClick={() => handlePrevNext('previous')}
                  >
                    ⏮
                  </button>
                  <button 
                    className="w-4 h-4 text-white/50 hover:text-white/80 hover:bg-white/10 rounded text-xs flex items-center justify-center transition-all"
                    onClick={() => handlePrevNext('next')}
                  >
                    ⏭
                  </button>
                </div>
              </div>

              {/* Compact Sync Controls */}
              <div className="space-y-1">
                <div className="text-xs text-white/50">Sync</div>
                <div className="flex items-center justify-center space-x-1">
                  <button onClick={() => adjustOffset(-0.5)} className="px-1 py-0.5 bg-red-500/20 hover:bg-red-500/30 text-red-200 rounded text-xs transition-all">-0.5</button>
                  <button onClick={() => adjustOffset(-0.1)} className="px-1 py-0.5 bg-red-500/15 hover:bg-red-500/25 text-red-200 rounded text-xs transition-all">-0.1</button>
                  <button onClick={() => resetOffset(false)} className="px-1 py-0.5 bg-white/10 hover:bg-white/20 text-white/70 rounded text-xs transition-all">0</button>
                  <button onClick={() => adjustOffset(0.1)} className="px-1 py-0.5 bg-green-500/15 hover:bg-green-500/25 text-green-200 rounded text-xs transition-all">+0.1</button>
                  <button onClick={() => adjustOffset(0.5)} className="px-1 py-0.5 bg-green-500/20 hover:bg-green-500/30 text-green-200 rounded text-xs transition-all">+0.5</button>
                </div>
                <div className="flex items-center justify-center space-x-1">
                  <span className="text-xs text-white/40">Global:</span>
                  <button onClick={() => adjustOffset(-0.2, true)} className="px-1 py-0.5 bg-orange-500/20 hover:bg-orange-500/30 text-orange-200 rounded text-xs transition-all">-0.2</button>
                  <span className="font-mono text-orange-300 text-xs px-1">{formatOffset(globalOffset)}</span>
                  <button onClick={() => adjustOffset(0.2, true)} className="px-1 py-0.5 bg-orange-500/20 hover:bg-orange-500/30 text-orange-200 rounded text-xs transition-all">+0.2</button>
                </div>
              </div>

              {/* Translation Controls */}
              <div className="space-y-1 pt-1 border-t border-white/10">
                <div className="text-xs text-white/50">Translation & Reading</div>
                <div className="flex justify-center gap-2">
                  <TranslationSettings className="text-xs" />
                  <HiraganaToggle />
                </div>
              </div>

              {/* Alignment Tester */}
              <div className="pt-1 border-t border-white/10">
                <div className="text-xs text-white/50 mb-1">Test Alignment</div>
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