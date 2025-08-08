// Simple WebSocket client test for debugging
// Using Node.js built-in WebSocket (available in Node.js 21+)
import { WebSocket } from 'ws';

console.log('🟡 [Test] Connecting to WebSocket server at ws://localhost:8765');

const ws = new WebSocket('ws://localhost:8765');

ws.on('open', () => {
    console.log('🟢 [Test] ✅ Connected to WebSocket server');
    
    // Send a test track message
    const trackMessage = {
        message_type: 'TRACK_DETECTED',
        data: {
            title: 'Test Song',
            artist: 'Test Artist',
            thumbnail: null,
            source: 'test',
            url: 'test://url',
            timestamp: Date.now(),
            is_playing: true,
            currentTime: 0,
            duration: 180
        },
        timestamp: Date.now()
    };
    
    console.log('🟡 [Test] Sending track message:', trackMessage);
    ws.send(JSON.stringify(trackMessage));
    
    // Send periodic pings
    setInterval(() => {
        if (ws.readyState === WebSocket.OPEN) {
            const pingMessage = {
                message_type: 'ping',
                data: {},
                timestamp: Date.now()
            };
            console.log('🟡 [Test] Sending ping');
            ws.send(JSON.stringify(pingMessage));
        }
    }, 10000);
});

ws.on('message', (data) => {
    try {
        const message = JSON.parse(data.toString());
        console.log('🟢 [Test] Received message:', message);
        
        if (message.message_type === 'PLAYBACK_COMMAND') {
            console.log('🟢 [Test] ✅ RECEIVED PLAYBACK COMMAND:', message.data);
            console.log('🟢 [Test] Command:', message.data.command);
            console.log('🟢 [Test] SeekTime:', message.data.seekTime);
        }
    } catch (error) {
        console.error('❌ [Test] Failed to parse message:', error);
        console.log('Raw message:', data.toString());
    }
});

ws.on('error', (error) => {
    console.error('❌ [Test] WebSocket error:', error);
});

ws.on('close', (code, reason) => {
    console.log('🔴 [Test] WebSocket closed:', code, reason.toString());
});

// Keep the process alive
setInterval(() => {
    console.log('🔵 [Test] Still running... WebSocket state:', ws.readyState);
}, 30000);