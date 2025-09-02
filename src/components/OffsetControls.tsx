import React from 'react';
import { motion } from 'framer-motion';
import { useOffsetStore } from '../stores/offsetStore';

interface OffsetControlsProps {
  artist: string;
  title: string;
  currentOffset?: number;
}

const OffsetControls: React.FC<OffsetControlsProps> = ({ 
  artist, 
  title, 
  currentOffset = 0 
}) => {
  const { 
    getTrackOffset, 
    setTrackOffset, 
    globalOffset, 
    setGlobalOffset,
    getTotalOffset,
    clearTrackOffset
  } = useOffsetStore();

  const trackOffset = getTrackOffset(artist, title);
  const totalOffset = getTotalOffset(artist, title);

  const adjustOffset = (delta: number, isGlobal = false) => {
    if (isGlobal) {
      const newGlobalOffset = globalOffset + delta;
      setGlobalOffset(newGlobalOffset);
      console.log(`[OffsetControls] Global offset adjusted by ${delta}s: ${globalOffset.toFixed(2)}s → ${newGlobalOffset.toFixed(2)}s`);
    } else {
      const newTrackOffset = trackOffset + delta;
      setTrackOffset(artist, title, newTrackOffset);
      console.log(`[OffsetControls] Track offset adjusted by ${delta}s: ${trackOffset.toFixed(2)}s → ${newTrackOffset.toFixed(2)}s for "${title}" by "${artist}"`);
    }
  };

  const resetOffset = (isGlobal = false) => {
    if (isGlobal) {
      setGlobalOffset(0);
    } else {
      clearTrackOffset(artist, title);
    }
  };

  const formatOffset = (offsetSeconds: number) => {
    const sign = offsetSeconds >= 0 ? '+' : '';
    return `${sign}${offsetSeconds.toFixed(1)}s`;
  };

  return (
    <div className="relative group">
      {/* Compact Trigger Button */}
      <button 
        className="p-1 text-white/40 hover:text-white/60 text-xs rounded hover:bg-white/5 transition-colors"
        title={`Sync: ${formatOffset(totalOffset)}`}
      >
        ⏱️
      </button>
      
      {/* Floating Panel */}
      <div className="absolute right-0 bottom-full mb-2 bg-black/95 backdrop-blur-lg rounded-lg border border-white/20 p-3 min-w-max opacity-0 group-hover:opacity-100 transition-opacity duration-300 pointer-events-none group-hover:pointer-events-auto">
        <div className="text-white/80 text-xs mb-2 text-center">
          Total: <span className="font-mono text-cyan-300">{formatOffset(totalOffset)}</span>
        </div>
        
        {/* Track Offset - Single Row */}
        <div className="flex items-center space-x-1 mb-2">
          <button
            onClick={() => adjustOffset(-0.5)}
            className="px-1.5 py-0.5 bg-red-500/20 hover:bg-red-500/30 text-white rounded text-xs transition-colors"
          >
            -0.5
          </button>
          <button
            onClick={() => adjustOffset(-0.1)}
            className="px-1 py-0.5 bg-red-500/15 hover:bg-red-500/25 text-white rounded text-xs transition-colors"
          >
            -0.1
          </button>
          <button
            onClick={() => resetOffset(false)}
            className="px-1 py-0.5 bg-gray-500/20 hover:bg-gray-500/30 text-white rounded text-xs transition-colors"
            title="Reset"
          >
            0
          </button>
          <button
            onClick={() => adjustOffset(0.1)}
            className="px-1 py-0.5 bg-green-500/15 hover:bg-green-500/25 text-white rounded text-xs transition-colors"
          >
            +0.1
          </button>
          <button
            onClick={() => adjustOffset(0.5)}
            className="px-1.5 py-0.5 bg-green-500/20 hover:bg-green-500/30 text-white rounded text-xs transition-colors"
          >
            +0.5
          </button>
        </div>
        
        {/* Global Offset Label + Controls */}
        <div className="flex items-center justify-between text-xs">
          <span className="text-white/60">Global:</span>
          <div className="flex items-center space-x-1">
            <button
              onClick={() => adjustOffset(-0.2, true)}
              className="px-1 py-0.5 bg-orange-500/20 hover:bg-orange-500/30 text-orange-200 rounded text-xs transition-colors"
            >
              -0.2
            </button>
            <span className="font-mono text-orange-300 px-1">{formatOffset(globalOffset)}</span>
            <button
              onClick={() => adjustOffset(0.2, true)}
              className="px-1 py-0.5 bg-orange-500/20 hover:bg-orange-500/30 text-orange-200 rounded text-xs transition-colors"
            >
              +0.2
            </button>
          </div>
        </div>
      </div>
    </div>
  );
};

export default OffsetControls;