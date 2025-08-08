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
      setGlobalOffset(globalOffset + delta);
    } else {
      setTrackOffset(artist, title, trackOffset + delta);
    }
  };

  const resetOffset = (isGlobal = false) => {
    if (isGlobal) {
      setGlobalOffset(0);
    } else {
      clearTrackOffset(artist, title);
    }
  };

  const formatOffset = (offset: number) => {
    const sign = offset >= 0 ? '+' : '';
    return `${sign}${(offset / 1000).toFixed(1)}s`;
  };

  return (
    <div className="bg-black/30 backdrop-blur-sm rounded-xl p-4 space-y-4">
      <div className="text-center">
        <h3 className="text-white font-semibold text-lg mb-2">Lyrics Sync</h3>
        <div className="text-white/70 text-sm">
          Total offset: <span className="font-mono text-cyan-400">{formatOffset(totalOffset * 1000)}</span>
        </div>
      </div>

      {/* Track-specific offset */}
      <div className="space-y-2">
        <div className="text-white/80 text-sm font-medium flex items-center justify-between">
          <span>Track Offset</span>
          <span className="font-mono text-cyan-300">{formatOffset(trackOffset * 1000)}</span>
        </div>
        
        <div className="flex items-center justify-center space-x-2">
          <motion.button
            whileHover={{ scale: 1.05 }}
            whileTap={{ scale: 0.95 }}
            onClick={() => adjustOffset(-1000)}
            className="px-3 py-2 bg-red-500/20 hover:bg-red-500/30 text-white rounded-lg transition-colors text-sm font-medium"
          >
            -1.0s
          </motion.button>
          
          <motion.button
            whileHover={{ scale: 1.05 }}
            whileTap={{ scale: 0.95 }}
            onClick={() => adjustOffset(-100)}
            className="px-2 py-2 bg-red-500/15 hover:bg-red-500/25 text-white rounded-lg transition-colors text-xs"
          >
            -0.1s
          </motion.button>

          <motion.button
            whileHover={{ scale: 1.05 }}
            whileTap={{ scale: 0.95 }}
            onClick={() => resetOffset(false)}
            className="px-3 py-2 bg-gray-500/20 hover:bg-gray-500/30 text-white rounded-lg transition-colors text-xs"
            title="Reset track offset"
          >
            Reset
          </motion.button>

          <motion.button
            whileHover={{ scale: 1.05 }}
            whileTap={{ scale: 0.95 }}
            onClick={() => adjustOffset(100)}
            className="px-2 py-2 bg-green-500/15 hover:bg-green-500/25 text-white rounded-lg transition-colors text-xs"
          >
            +0.1s
          </motion.button>
          
          <motion.button
            whileHover={{ scale: 1.05 }}
            whileTap={{ scale: 0.95 }}
            onClick={() => adjustOffset(1000)}
            className="px-3 py-2 bg-green-500/20 hover:bg-green-500/30 text-white rounded-lg transition-colors text-sm font-medium"
          >
            +1.0s
          </motion.button>
        </div>
      </div>

      {/* Global offset */}
      <div className="space-y-2 pt-2 border-t border-white/10">
        <div className="text-white/80 text-sm font-medium flex items-center justify-between">
          <span>Global Offset</span>
          <span className="font-mono text-orange-300">{formatOffset(globalOffset * 1000)}</span>
        </div>
        
        <div className="flex items-center justify-center space-x-2">
          <motion.button
            whileHover={{ scale: 1.05 }}
            whileTap={{ scale: 0.95 }}
            onClick={() => adjustOffset(-500, true)}
            className="px-2 py-1 bg-orange-500/20 hover:bg-orange-500/30 text-white rounded text-xs"
          >
            -0.5s
          </motion.button>

          <motion.button
            whileHover={{ scale: 1.05 }}
            whileTap={{ scale: 0.95 }}
            onClick={() => resetOffset(true)}
            className="px-2 py-1 bg-gray-500/20 hover:bg-gray-500/30 text-white rounded text-xs"
            title="Reset global offset"
          >
            Reset
          </motion.button>

          <motion.button
            whileHover={{ scale: 1.05 }}
            whileTap={{ scale: 0.95 }}
            onClick={() => adjustOffset(500, true)}
            className="px-2 py-1 bg-orange-500/20 hover:bg-orange-500/30 text-white rounded text-xs"
          >
            +0.5s
          </motion.button>
        </div>
      </div>

      {/* Help text */}
      <div className="text-white/50 text-xs text-center leading-relaxed">
        <div>Track offset applies to this song only</div>
        <div>Global offset applies to all songs</div>
        <div className="mt-1 text-white/40">
          If lyrics are ahead of audio, use negative offset
        </div>
      </div>
    </div>
  );
};

export default OffsetControls;